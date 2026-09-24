import json
from pathlib import Path
import tempfile
import unittest
import urllib.error
from unittest.mock import patch
import workspace_ai as ai


class AITests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.data = Path(self.temp.name)
        self.key = 'fixture-key-not-a-real-credential'
        ai.save(self.data, {'api_key': self.key})
        with patch.object(ai, 'request', return_value={'data': [{'id': 'openai/gpt-4.1-mini'}]}):
            ai.fetch_models(self.data)

    def tearDown(self):
        self.temp.cleanup()

    def test_secret_is_not_returned_and_database_is_private(self):
        self.assertNotIn(self.key, json.dumps(ai.public(self.data)))
        self.assertEqual((self.data / 'workspace-ai.sqlite').stat().st_mode & 0o777, 0o600)
        self.assertEqual(ai.settings(self.data)['key'], self.key)
        ai.save(self.data, {'model': 'openai/gpt-4.1-mini'})
        self.assertEqual(ai.settings(self.data)['key'], self.key)
        ai.clear(self.data)
        self.assertFalse(ai.public(self.data)['configured'])

    def test_provider_error_cannot_echo_secret(self):
        class Broken:
            def open(self, *args, **kwargs):
                raise urllib.error.HTTPError('https://api.timeweb.ai/v1/models', 401, self.key, {}, None)
        broken = Broken()
        broken.key = self.key
        with patch.object(ai.urllib.request, 'build_opener', return_value=broken):
            with self.assertRaises(ai.AIError) as error:
                ai.request(self.key, '/models')
        self.assertNotIn(self.key, str(error.exception))

    def test_analysis_transmits_only_anonymous_snapshot_and_no_tools(self):
        import workspace_monitor as monitor
        private = {'previous': None, 'current': {'id': 7, 'projects': [{'id': 'internal-id', 'name': 'PRIVATE PROJECT'}]}}
        anonymous = {'snapshots': [{'projects': [{'id': 'project-1', 'size': 1000}], 'repositories': []}]}
        report = {'headline': 'Короткий вывод', 'observations': ['project-1 занимает 1 KB'],
                  'actions': ['Проверить локальные изменения'], 'risks': ['Нет второго снимка'], 'reasoning': 'Факты снимка.'}
        with patch.object(monitor, 'anonymous', return_value=anonymous), patch.object(ai, 'request', return_value={
            'choices': [{'message': {'content': json.dumps(report)}}], 'usage': {'total_tokens': 123}}) as request:
            result = ai.analyze(self.data, private)
        sent = request.call_args.args[2]
        self.assertNotIn('PRIVATE PROJECT', json.dumps(sent))
        self.assertNotIn(self.key, json.dumps(sent))
        self.assertNotIn('tools', sent)
        self.assertEqual(result['result']['status'], 'complete')
        self.assertEqual(result['result']['report']['headline'], report['headline'])
        self.assertEqual(result['result']['labels']['project-1']['name'], 'PRIVATE PROJECT')
        self.assertEqual(ai.public(self.data, None, 7)['result']['usage']['total_tokens'], 123)

    def test_key_rotation_resets_catalog_and_unlisted_models_are_rejected(self):
        with self.assertRaises(ai.AIError):
            ai.save(self.data, {'model': 'unknown-model'})
        result = ai.save(self.data, {'api_key': 'another-fixture-credential'})
        self.assertEqual(result['models'], [])
        self.assertEqual(result['model'], '')


if __name__ == '__main__':
    unittest.main()
