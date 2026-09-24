import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('wm', Path(__file__).with_name('workspace_monitor.py'))
wm = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wm)


class WorkspaceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.data = self.root / 'data'
        self.data.mkdir()
        self.repo = self.root / 'projects' / 'sensitive-project-name'
        self.repo.mkdir(parents=True)
        self.cmd('init', '-b', 'main')
        self.cmd('config', 'user.name', 'Fixture')
        self.cmd('config', 'user.email', 'fixture@example.invalid')
        (self.repo / 'source.txt').write_text('tracked\n')
        self.cmd('add', '.')
        self.cmd('commit', '-m', 'fixture')
        wm.atomic(self.data / 'workspace-config.json', {'roots': [str(self.repo.parent)], 'interval_hours': 6})

    def tearDown(self):
        self.temp.cleanup()

    def cmd(self, *args):
        return subprocess.check_output(['/usr/bin/git', '-C', str(self.repo), *args], stderr=subprocess.DEVNULL).decode().strip()

    def test_linked_worktrees_shared_git_and_read_only(self):
        worktree = self.repo.parent / 'branch-copy'
        self.cmd('worktree', 'add', '-b', 'draft', str(worktree))
        self.cmd('worktree', 'lock', str(worktree), '--reason', 'keep this work')
        (worktree / 'source.txt').write_text('changed\n')
        (self.repo / 'untracked.txt').write_text('precious')
        deps = self.repo / 'node_modules' / 'package'
        deps.mkdir(parents=True)
        (deps / 'large.bin').write_bytes(b'x' * 16384)
        # Symlinks are not followed. The dependency subtree cannot create fake projects.
        (deps / '.git').mkdir()
        (self.repo / 'linked').symlink_to(self.root)
        before = self.cmd('show-ref')
        index_before = (self.repo / '.git/index').read_bytes()
        with patch.object(wm, 'docker_snapshot', return_value={'status': 'unavailable'}):
            wm.scan(self.data)
        value = wm.view(self.data)
        snapshot = value['current']
        self.assertEqual(len(snapshot['repositories']), 1)
        self.assertEqual(len(snapshot['projects']), 2)
        self.assertGreater(snapshot['summary']['dependencies'], 0)
        self.assertGreater(snapshot['summary']['git_bytes'], 0)
        self.assertEqual(snapshot['summary']['dirty_count'], 2)
        self.assertTrue(any('locked' in w for w in snapshot['repositories'][0]['worktrees']))
        self.assertEqual(before, self.cmd('show-ref'))
        self.assertEqual(index_before, (self.repo / '.git/index').read_bytes())
        self.assertFalse(value['comparable'])
        self.assertTrue((self.repo / 'untracked.txt').exists())

    def test_history_scope_and_export_allowlist(self):
        with patch.object(wm, 'docker_snapshot', return_value={'status': 'unavailable'}):
            wm.scan(self.data)
            (self.repo / 'new-file').write_bytes(b'y' * 65536)
            wm.scan(self.data)
        value = wm.view(self.data)
        self.assertTrue(value['comparable'])
        self.assertGreater(value['current']['summary']['total'], value['previous']['summary']['total'])
        exported = json.dumps(wm.anonymous(value))
        for forbidden in [str(self.repo), self.repo.name, self.cmd('rev-parse', 'HEAD'), 'source.txt']:
            self.assertNotIn(forbidden, exported)
        with wm.database(self.data) as db:
            current = value['current']
            current['scope'] = 'different'
            db.execute('UPDATE snapshots SET scope=?,data=? WHERE id=?', ('different', json.dumps(current), current['id']))
        self.assertFalse(wm.view(self.data, before=value['previous']['id'])['comparable'])

    def test_remote_confirmation_never_uses_cached_refs_as_proof(self):
        branches = [dict(name='main', sha='a' * 40, remote='origin', remote_ref='refs/heads/main'),
                    dict(name='draft', sha='b' * 40, remote='', remote_ref='')]
        with patch.object(wm, 'git', return_value='a' * 40 + '\trefs/heads/main\n') as command:
            wm.verify_branches(self.repo, branches)
        self.assertEqual(branches[0]['published'], 'confirmed')
        self.assertEqual(branches[1]['published'], 'no_upstream')
        self.assertEqual(command.call_args.args[1], 'ls-remote')
        with patch.object(wm, 'git', return_value=None):
            wm.verify_branches(self.repo, branches)
        self.assertEqual(branches[0]['published'], 'unavailable')

    def test_unknown_remote_objects_and_dirty_rename(self):
        branch = dict(name='main', sha='a'*40, remote='origin', remote_ref='refs/heads/main')
        with patch.object(wm, 'git', side_effect=['b'*40 + '\trefs/heads/main\n', None]):
            wm.verify_branches(self.repo, [branch])
        self.assertEqual(branch['published'], 'unknown_tip')
        self.cmd('mv', 'source.txt', 'renamed.txt')
        state = wm.project_state(self.repo)
        self.assertEqual(state['changed'], 1)
        self.assertEqual(state['untracked'], 0)

    def test_cancellation_does_not_replace_completed_history(self):
        with patch.object(wm, 'docker_snapshot', return_value={'status': 'unavailable'}):
            wm.scan(self.data)
        sid = wm.view(self.data)['current']['id']
        (self.data / 'workspace-cancel').touch()
        with self.assertRaises(wm.Cancelled):
            wm.scan(self.data)
        self.assertEqual(wm.view(self.data)['current']['id'], sid)


if __name__ == '__main__':
    unittest.main()
