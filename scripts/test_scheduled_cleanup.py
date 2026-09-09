import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import urllib.error

import resume_spinitron_cleanup as worker


class ScheduledCleanupTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name) / 'cooldown.json'
        self.client = worker.PacedSpotify('id', 'secret', 'refresh', cooldown_path=self.path)
        self.client.token, self.client.expires = 'test', float('inf')

    def test_each_attempt_obeys_shared_spacing_including_get_retries(self):
        clock = [100.0]
        starts = []
        def sleep(seconds):
            clock[0] += seconds
        def request(_request, timeout):
            starts.append(clock[0])
            if len(starts) == 1:
                raise urllib.error.HTTPError('url', 503, 'unavailable', {}, None)
            return io.BytesIO(b'{}')
        with patch.object(worker.time, 'monotonic', side_effect=lambda: clock[0]), patch.object(worker.time, 'sleep', side_effect=sleep), patch.object(worker.cleanup.urllib.request, 'urlopen', side_effect=request):
            self.client.request('GET', '/me')
            self.client.request('GET', '/me')
        self.assertEqual(starts, [100.0, 102.0, 104.0])

    def test_429_is_persisted_and_prevents_any_retry_or_next_call(self):
        error = urllib.error.HTTPError('url', 429, 'limited', {'Retry-After':'500'}, None)
        with patch.object(worker.time, 'time', return_value=1000), patch.object(worker.time, 'sleep'), patch.object(worker.cleanup.urllib.request, 'urlopen', side_effect=error) as request:
            with self.assertRaises(worker.CooldownActive):
                self.client.request('GET', '/me')
            with self.assertRaises(worker.CooldownActive):
                self.client.request('GET', '/me')
            self.assertEqual(request.call_count, 1)
        self.assertEqual(json.loads(self.path.read_text())['not_before'], 1560)
        restored = worker.PacedSpotify('id', 'secret', 'refresh', cooldown_path=self.path)
        self.assertEqual(restored.cooldown_until, 1560)

    def test_active_cooldown_returns_before_auth_or_network(self):
        with patch.object(worker, 'read_cooldown', return_value=float('inf')), patch.object(worker, 'authorize') as auth, patch.object(worker.cleanup, 'PersonalGitHub') as github, patch('builtins.print'):
            # A finite timestamp is needed solely for the formatted status output.
            with patch.object(worker, 'read_cooldown', return_value=9999999999):
                worker.main()
        auth.assert_not_called()
        github.assert_not_called()
