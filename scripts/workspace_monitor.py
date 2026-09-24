#!/usr/bin/env python3
"""Read-only workspace inventory. No Git/Docker/filesystem cleanup commands."""
import argparse
import hashlib
import fcntl
import json
import os
from pathlib import Path
import signal
import sqlite3
import stat
import subprocess
import tempfile
import time
import urllib.request

SCHEMA = 1
SKIP = {'.git', 'node_modules', '.venv', 'venv', '.next', 'dist', 'build',
        '.cache', '.turbo', '.toolchain', 'target', 'Pods', '.build', '__pycache__'}
DEPENDENCIES = {'node_modules', '.venv', 'venv', 'Pods', '.toolchain'}
BUILDS = {'dist', 'build', '.next', 'target', '.build', 'DerivedData'}
CACHES = {'.cache', '.turbo', '__pycache__', '.pytest_cache', '.mypy_cache'}


def identity(value):
    return hashlib.sha256(str(value).encode()).hexdigest()[:20]


def atomic(path, value):
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(value, ensure_ascii=False))
    os.chmod(temporary, 0o600)
    os.replace(temporary, path)


def load(path, default):
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return default


def configuration(data):
    return load(data / 'workspace-config.json', {
        'roots': [str(Path.home() / 'Documents/ChatGPT'), str(Path.home() / 'Desktop/UNIUM DEV')],
        'interval_hours': 6})


def validated_roots(roots):
    if not isinstance(roots, list) or not 1 <= len(roots) <= 16:
        raise ValueError('Укажите от 1 до 16 папок.')
    home = Path.home().resolve()
    paths = []
    for value in roots:
        if not isinstance(value, str) or len(value) > 4096:
            raise ValueError('Некорректный путь.')
        path = Path(value).expanduser().resolve()
        if not path.is_dir() or (path != home and home not in path.parents):
            raise ValueError('Выберите существующую папку внутри домашней папки.')
        if path not in paths:
            paths.append(path)
    # A nested root is already covered by its parent.
    return [str(p) for p in sorted(paths) if not any(q in p.parents for q in paths)]


def database(data):
    db = sqlite3.connect(str(data / 'development.sqlite'), timeout=10)
    db.execute('PRAGMA journal_mode=WAL')
    db.execute('''CREATE TABLE IF NOT EXISTS snapshots (
        id INTEGER PRIMARY KEY AUTOINCREMENT, ts REAL NOT NULL,
        scope TEXT NOT NULL, status TEXT NOT NULL, data TEXT NOT NULL)''')
    db.execute('CREATE INDEX IF NOT EXISTS snapshots_time ON snapshots(ts)')
    return db


class Cancelled(Exception):
    pass


class Worker:
    def __init__(self, data):
        self.data = data
        self.started = time.time()
        self.visited = 0
        self.errors = 0
        self.last_publish = 0
        self.phase = 'Поиск проектов'
        self.current = ''
        self.found = 0

    def check(self, force=False):
        if (self.data / 'workspace-cancel').exists():
            raise Cancelled()
        now = time.time()
        if force or now - self.last_publish > 1:
            atomic(self.data / 'workspace-status.json', {
                'status': 'scanning', 'started': self.started, 'updated': now,
                'phase': self.phase, 'current': self.current, 'visited': self.visited,
                'found': self.found, 'errors': self.errors})
            self.last_publish = now

    def step(self):
        self.visited += 1
        if self.visited % 200 == 0:
            self.check()
            time.sleep(.005)


def git(path, *args, timeout=12):
    """Read-only commands supplied by this module; bounded output, no optional locks."""
    env = os.environ.copy()
    env.update(GIT_OPTIONAL_LOCKS='0', GIT_TERMINAL_PROMPT='0', GIT_ASKPASS='/usr/bin/false',
               GIT_NO_LAZY_FETCH='1', GIT_NO_REPLACE_OBJECTS='1', GIT_GRAFT_FILE='/dev/null',
               SSH_ASKPASS='/usr/bin/false', LC_ALL='C',
               GIT_SSH_COMMAND='/usr/bin/ssh -oBatchMode=yes -oStrictHostKeyChecking=yes -oConnectTimeout=8')
    command = ['/usr/bin/git', '--no-optional-locks', '-c', 'core.fsmonitor=false',
               '-c', 'core.untrackedCache=false', '-c', 'core.hooksPath=/dev/null',
               '-c', 'core.quotePath=false', '-c', 'protocol.allow=never',
               '-c', 'protocol.https.allow=always', '-c', 'protocol.ssh.allow=always',
               '-C', str(path), *args]
    with tempfile.TemporaryFile() as out:
        try:
            process = subprocess.Popen(command, env=env, stdout=out, stderr=subprocess.DEVNULL,
                                       start_new_session=True)
            try:
                returncode = process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                # Only this helper's own process group, including its SSH child.
                try:
                    os.killpg(process.pid, signal.SIGTERM)
                    process.wait(timeout=1)
                except (ProcessLookupError, subprocess.TimeoutExpired):
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    process.wait()
                return None
            if out.tell() > 4 * 1024 * 1024:
                return None
            out.seek(0)
            value = out.read().decode('utf-8', 'replace')
            return value if returncode == 0 else None
        except (OSError, subprocess.TimeoutExpired):
            return None


def discover(roots, worker):
    found, seen = [], set()
    for root in roots:
        root = Path(root)
        if not root.is_dir():
            worker.errors += 1
            continue
        device = root.stat().st_dev
        pending = [(root, 0)]
        while pending:
            path, depth = pending.pop()
            worker.step()
            if str(path) in seen:
                continue
            seen.add(str(path))
            worker.current = str(path)
            try:
                if (path / '.git').exists():
                    found.append(path)
                    worker.found = len(found)
                with os.scandir(path) as entries:
                    for entry in entries:
                        if entry.name in SKIP or not entry.is_dir(follow_symlinks=False):
                            continue
                        if entry.stat(follow_symlinks=False).st_dev != device:
                            continue
                        if depth >= 128:
                            worker.errors += 1
                        else:
                            pending.append((Path(entry.path), depth + 1))
            except OSError:
                worker.errors += 1
    return sorted(set(found))


def branch_records(path):
    raw = git(path, 'for-each-ref', '--format=%(refname:short)%00%(objectname)%00%(upstream:short)%00%(upstream:track)%00%(committerdate:unix)%00%(upstream:remotename)%00%(upstream:remoteref)', 'refs/heads/')
    if raw is None:
        return None
    rows = []
    for line in raw.splitlines():
        parts = line.split('\0')
        if len(parts) == 7:
            name, sha, upstream, track, date, remote, remote_ref = parts
            rows.append(dict(name=name, sha=sha, upstream=upstream, track=track,
                             committed=int(date or 0), remote=remote, remote_ref=remote_ref,
                             published='not_checked'))
    return rows


def verify_branches(path, branches, worker=None):
    remotes = {}
    for branch in branches:
        if worker:
            worker.check()
        remote = branch['remote']
        if not remote or remote == '.':
            branch['published'] = 'no_upstream'
            continue
        if remote not in remotes:
            # No fetch: remote refs are read, not written into the local repository.
            raw = git(path, 'ls-remote', '--heads', '--', remote, timeout=25)
            remotes[remote] = None if raw is None else {
                line.split('\t', 1)[1]: line.split('\t', 1)[0]
                for line in raw.splitlines() if '\t' in line}
        refs = remotes[remote]
        branch['verified_at'] = time.time()
        if refs is None:
            branch['published'] = 'unavailable'
            continue
        tip = refs.get(branch['remote_ref'])
        if not tip:
            branch['published'] = 'missing_ref'
        elif tip == branch['sha']:
            branch['published'] = 'confirmed'
        elif git(path, 'cat-file', '-e', tip + '^{commit}') is None:
            branch['published'] = 'unknown_tip'
        elif git(path, 'merge-base', '--is-ancestor', branch['sha'], tip) is not None:
            branch['published'] = 'confirmed'
        else:
            branch['published'] = 'not_reachable'


def worktree_records(path):
    raw = git(path, 'worktree', 'list', '--porcelain', '-z')
    if raw is None:
        return []
    records, current = [], {}
    for field in raw.split('\0'):
        if not field:
            if current:
                records.append(current)
                current = {}
            continue
        key, _, value = field.partition(' ')
        current[key] = value or True
    if current:
        records.append(current)
    return records


def project_state(path):
    raw = git(path, 'status', '--porcelain=v2', '-z', '--branch', '--untracked-files=normal')
    counts = {'changed': 0, 'untracked': 0, 'conflicts': 0}
    branch, head, ahead, behind = '', '', None, None
    if raw is not None:
        records = iter(raw.split('\0'))
        for record in records:
            if record.startswith('# branch.head '):
                branch = record[14:]
            elif record.startswith('# branch.oid '):
                head = record[13:]
            elif record.startswith('# branch.ab '):
                ab = record.split()
                ahead, behind = int(ab[-2][1:]), int(ab[-1][1:])
            elif record.startswith(('1 ', '2 ')):
                counts['changed'] += 1
                if record.startswith('2 '):
                    next(records, None)  # rename's original path is a separate NUL field
            elif record.startswith('? '):
                counts['untracked'] += 1
            elif record.startswith('u '):
                counts['conflicts'] += 1
    return dict(branch=branch, head=head, ahead=ahead, behind=behind, git_ok=raw is not None,
                git_at=time.time(), **counts)


def measure(root, excluded, worker, seen):
    sizes = dict(source=0, dependencies=0, build=0, cache=0)
    errors = 0
    pending = [(root, 'source', 0)]
    try:
        device = root.stat().st_dev
    except OSError:
        return sizes, 1
    while pending:
        path, category, depth = pending.pop()
        worker.step()
        try:
            info = path.lstat()
            if info.st_dev != device or stat.S_ISLNK(info.st_mode):
                continue
            if path != root and path in excluded:
                continue
            if stat.S_ISDIR(info.st_mode):
                if path.name == '.git' and path != root:
                    continue
                if depth > 128:
                    errors += 1
                    continue
                if category == 'source':
                    if path.name in DEPENDENCIES:
                        category = 'dependencies'
                    elif path.name in BUILDS:
                        category = 'build'
                    elif path.name in CACHES:
                        category = 'cache'
                with os.scandir(path) as entries:
                    pending.extend((Path(e.path), category, depth + 1) for e in entries)
            elif stat.S_ISREG(info.st_mode):
                if path.name == '.git':
                    continue
                inode = (info.st_dev, info.st_ino)
                if info.st_nlink > 1:
                    if inode in seen:
                        continue
                    seen.add(inode)
                sizes[category] += info.st_blocks * 512
        except OSError:
            errors += 1
    return sizes, errors


def docker_snapshot():
    try:
        # Do not inherit HTTP_PROXY for the local monitor endpoint.
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        with opener.open('http://127.0.0.1:9899/api/manage/docker', timeout=60) as response:
            raw = response.read(8 * 1024 * 1024 + 1)
            if len(raw) > 8 * 1024 * 1024:
                raise ValueError()
            value = json.loads(raw)
        if value.get('status') != 'available':
            return {'status': 'unavailable', 'ts': time.time()}
        # No names, labels, mounts, env, image commands or host paths in snapshots.
        def valid(n):
            return n if isinstance(n, (int, float)) and n >= 0 else None
        groups = {
            'containers': [dict(id=identity(c['Id']), size=valid(c.get('SizeRw')), state=c.get('State'),
                                finished=c.get('FinishedAt')) for c in (value.get('containers') or [])],
            'images': [dict(id=identity(i['Id']), size=valid(i.get('Size')), used=i.get('Containers'))
                       for i in (value.get('images') or [])],
            'volumes': [dict(id=identity(v['Name']), size=valid((v.get('UsageData') or {}).get('Size')),
                            refs=(v.get('UsageData') or {}).get('RefCount')) for v in (value.get('volumes') or [])],
            'cache': [dict(id=identity(c['ID']), size=valid(c.get('Size')), in_use=c.get('InUse'),
                          last_used=c.get('LastUsedAt')) for c in (value.get('cache') or [])]}
        disk = value.get('desktop_disk') or {}
        return dict(status='available', ts=value.get('ts', time.time()),
                    engine=identity(value['engine_id']) if value.get('engine_id') else None,
                    allocated=valid(disk.get('allocated')), layers=valid(value.get('layers_size')), **groups)
    except (OSError, ValueError, KeyError, TypeError):
        return {'status': 'unavailable', 'ts': time.time()}


def scan(data, verify=False):
    worker = Worker(data)
    worker.check(True)
    config = configuration(data)
    roots = sorted({str(Path(r).expanduser().resolve()) for r in config['roots']})
    projects, repositories = [], {}
    try:
        paths = discover(roots, worker)
        for path in paths:
            worker.current = str(path)
            worker.phase = 'Состояние Git и веток'
            worker.check(True)
            common = git(path, 'rev-parse', '--path-format=absolute', '--git-common-dir')
            if common is None:
                worker.errors += 1
                continue
            common = Path(common.strip()).resolve()
            rid = identity(common)
            if rid not in repositories:
                branches = branch_records(path)
                stashes = git(path, 'stash', 'list', '--format=%H')
                if verify and branches is not None:
                    worker.phase = 'Сверка коммитов с remote'
                    worker.check(True)
                    verify_branches(path, branches, worker)
                repositories[rid] = dict(id=rid, common_dir=str(common), branches=branches,
                    stashes=len(stashes.splitlines()) if stashes is not None else None,
                    worktrees=worktree_records(path), git_bytes=None, git_complete=False)
            projects.append(dict(id=identity(path), repo_id=rid, name=path.name, path=str(path),
                                 **project_state(path)))
        worker.phase = 'Размеры рабочих копий'
        all_paths = {Path(p['path']) for p in projects}
        common_paths = {Path(r['common_dir']) for r in repositories.values()}
        seen = set()
        for project in projects:
            path = Path(project['path'])
            worker.current = str(path)
            worker.check(True)
            sizes, errors = measure(path, all_paths | common_paths, worker, seen)
            project.update(sizes=sizes, size=sum(sizes.values()), complete=errors == 0, errors=errors)
            worker.errors += errors
        worker.phase = 'Общие данные Git'
        for repo in repositories.values():
            path = Path(repo['common_dir'])
            # Linked repositories outside the configured roots are metadata-only.
            if not any(path == Path(r) or Path(r) in path.parents for r in roots):
                repo['outside_scope'] = True
                continue
            worker.current = str(path)
            worker.check(True)
            sizes, errors = measure(path, common_paths | all_paths, worker, seen)
            repo.update(git_bytes=sum(sizes.values()), git_complete=errors == 0)
            worker.errors += errors
        worker.phase = 'Снимок Docker'
        worker.current = ''
        worker.check(True)
        docker = docker_snapshot()
        worker.check(True)
        disk = os.statvfs(str(Path.home()))
        summary = dict(project_bytes=sum(p['size'] for p in projects),
                       git_bytes=sum(r['git_bytes'] or 0 for r in repositories.values()),
                       project_count=len(projects), repo_count=len(repositories),
                       branch_count=sum(len(r['branches'] or []) for r in repositories.values()),
                       dirty_count=sum(bool(p['changed'] or p['untracked'] or p['conflicts']) for p in projects),
                       git_unknown=sum(not p['git_ok'] for p in projects),
                       disk_free=disk.f_bavail * disk.f_frsize,
                       docker_allocated=docker.get('allocated'))
        for key in ('source', 'dependencies', 'build', 'cache'):
            summary[key] = sum(p['sizes'][key] for p in projects)
        summary['total'] = summary['project_bytes'] + summary['git_bytes']
        complete = worker.errors == 0 and all(r['git_complete'] for r in repositories.values())
        snapshot = dict(schema=SCHEMA, scope=identity('\0'.join(roots)), roots=roots,
                        started=worker.started, ts=time.time(), status='complete' if complete else 'partial',
                        errors=worker.errors, summary=summary, projects=projects,
                        repositories=list(repositories.values()), docker=docker, verify_remote=verify)
        with database(data) as db:
            cursor = db.execute('INSERT INTO snapshots(ts,scope,status,data) VALUES(?,?,?,?)',
                                (snapshot['ts'], snapshot['scope'], snapshot['status'], json.dumps(snapshot)))
            sid = cursor.lastrowid
            db.execute('DELETE FROM snapshots WHERE ts < ? OR id NOT IN (SELECT id FROM snapshots ORDER BY id DESC LIMIT 1000)',
                       (time.time() - 90 * 86400,))
        atomic(data / 'workspace-status.json', dict(status='idle', finished=time.time(), snapshot_id=sid,
                                                  visited=worker.visited, errors=worker.errors))
    except Cancelled:
        atomic(data / 'workspace-status.json', dict(status='cancelled', finished=time.time(), visited=worker.visited))
    except Exception:
        atomic(data / 'workspace-status.json', dict(status='error', finished=time.time(),
                message='Снимок не сохранён. Проверьте доступ к папкам и повторите запуск.'))
        raise


def view(data, before=None, after=None):
    with database(data) as db:
        rows = db.execute('SELECT id,ts,scope,status,json_extract(data,\'$.summary\') FROM snapshots ORDER BY id DESC').fetchall()
        def get(sid):
            row = db.execute('SELECT data FROM snapshots WHERE id=?', (sid,)).fetchone()
            return dict(json.loads(row[0]), id=sid) if row else None
        after = after or (rows[0][0] if rows else None)
        current = get(after) if after else None
        if after is not None and current is None:
            raise ValueError('Snapshot not found')
        if before is None and current:
            before = next((r[0] for r in rows if r[0] < after and r[2] == current['scope']), None)
        previous = get(before) if before else None
        if before is not None and previous is None:
            raise ValueError('Snapshot not found')
        history = [dict(id=r[0], ts=r[1], scope=r[2], status=r[3], summary=json.loads(r[4])) for r in rows]
    import workspace_ai
    gateway = workspace_ai.public(data, previous['id'] if previous else None, current['id'] if current else None)
    return dict(config=configuration(data), gateway=gateway, status=load(data / 'workspace-status.json', {'status': 'idle'}),
                snapshots=history, current=current, previous=previous,
                comparable=bool(current and previous and current['scope'] == previous['scope']
                                and current['status'] == previous['status'] == 'complete'))


def anonymous(view_value):
    """Strict allowlist: no arbitrary Git/Docker strings or hashes leave the snapshot."""
    result = {'schema': SCHEMA, 'purpose': 'read_only_capacity_review',
              'limits': ['No deletion authorization', 'APFS blocks are not reclaimable space',
                         'Remote evidence covers commits, not working files or stash'],
              'comparable': view_value['comparable'], 'field_definitions': {'dirty_worktrees': 'Number of WORKING COPIES with local changes, NOT a number of files', 'changed': 'Changed tracked paths in one working copy', 'untracked': 'Untracked entries; an entire directory may count as one', 'stashes': 'Saved stash entries; not known to be published'}, 'snapshots': []}
    before, after = view_value['previous'], view_value['current']
    result['docker_comparable'] = bool(before and after and before['docker'].get('engine')
        and before['docker'].get('engine') == after['docker'].get('engine'))
    ids = {}
    for snapshot in (view_value['previous'], view_value['current']):
        if not snapshot:
            continue
        projects = []
        for p in snapshot['projects']:
            alias = ids.setdefault(p['id'], 'project-' + str(len(ids) + 1))
            projects.append({**{k: p[k] for k in ('size', 'sizes', 'complete', 'changed', 'untracked', 'conflicts', 'git_ok', 'ahead', 'behind')}, 'id': alias})
        repo_stats = []
        for r in snapshot['repositories']:
            published = {}
            for b in r['branches'] or []:
                state = b['published']
                published[state] = published.get(state, 0) + 1
            repo_stats.append(dict(git_bytes=r['git_bytes'], stashes=r['stashes'], branches=len(r['branches'] or []), publication=published))
        docker = snapshot['docker']
        docker_summary = {k: docker.get(k) for k in ('status', 'allocated', 'layers')}
        for group in ('containers', 'images', 'volumes', 'cache'):
            objects = docker.get(group)
            docker_summary[group] = None if objects is None else dict(count=len(objects),
                measured_bytes=sum(o['size'] or 0 for o in objects), unknown=sum(o['size'] is None for o in objects))
        summary = dict(snapshot['summary'])
        summary['dirty_worktrees'] = summary.pop('dirty_count')
        result['snapshots'].append(dict(ts=snapshot['ts'], status=snapshot['status'], summary=summary,
                                        projects=projects, repositories=repo_stats, docker=docker_summary))
    return result


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser()
    parser.add_argument('command', choices=['scan', 'view', 'export', 'settings', 'ai-save', 'ai-models', 'ai-clear', 'ai-analyze', 'ai-status'])
    parser.add_argument('--data', type=Path, required=True)
    parser.add_argument('--before', type=int)
    parser.add_argument('--after', type=int)
    parser.add_argument('--verify-remote', action='store_true')
    args = parser.parse_args()
    args.data.mkdir(parents=True, exist_ok=True)
    if args.command.startswith('ai-'):
        import sys
        import workspace_ai as ai
        try:
            if args.command == 'ai-save':
                result = ai.save(args.data, json.loads(sys.stdin.read(65536)))
            elif args.command == 'ai-models':
                result = ai.fetch_models(args.data)
            elif args.command == 'ai-clear':
                result = ai.clear(args.data)
            elif args.command == 'ai-analyze':
                result = ai.analyze(args.data, view(args.data, args.before, args.after))
            else:
                result = ai.public(args.data)
            print(json.dumps(result, ensure_ascii=False))
        except ai.AIError as error:
            print(json.dumps({'error': str(error)}, ensure_ascii=False))
    elif args.command == 'scan':
        # Survives agent restarts: at most one worker can write a snapshot.
        lock = (args.data / 'workspace.lock').open('a')
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise SystemExit(75)
        (args.data / 'workspace-cancel').unlink(missing_ok=True)
        try:
            os.nice(10)
        except OSError:
            pass
        scan(args.data, args.verify_remote)
    elif args.command == 'settings':
        import sys
        value = json.loads(sys.stdin.read(65536))
        config = {'roots': validated_roots(value.get('roots')), 'interval_hours': 6}
        atomic(args.data / 'workspace-config.json', config)
        print(json.dumps(config))
    else:
        value = view(args.data, args.before, args.after)
        print(json.dumps(anonymous(value) if args.command == 'export' else value, ensure_ascii=False))


if __name__ == '__main__':
    main()
