import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch

import run_macos_comparison as comparison


class ReadinessTests(unittest.TestCase):
    def test_launch_waits_for_readiness_and_keeps_requested_lifetime(self):
        process = Mock(pid=42)
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / 'app.log'
            with patch.object(comparison.preflight, 'dialogs', return_value=[]), \
                    patch.object(comparison, 'stage', side_effect=lambda p: Path('/tmp') / Path(p).name), \
                    patch.object(comparison.subprocess, 'Popen', return_value=process) as spawn, \
                    patch.object(comparison, 'run', return_value='{"ready":true}') as probe, \
                    patch.object(comparison.time, 'monotonic', side_effect=[100, 102]):
                self.assertIs(comparison.launch(Path('app'), 'scroll', 21, log), process)
            probe.assert_called_once_with([str(comparison.READY), '42', str(comparison.READY_TIMEOUT)])
            self.assertEqual(spawn.call_args.kwargs['env']['ZGX_SECONDS'], '21')
            self.assertEqual(process.benchmark_started, 100)
            self.assertEqual(process.benchmark_ready_seconds, 2)
            self.assertEqual(json.loads(log.with_suffix('.ready.json').read_text())['spawn_to_ready_seconds'], 2)

    def test_failed_readiness_preserves_error_and_ends_only_the_child(self):
        process = Mock(pid=42)
        process.poll.return_value = None
        error = subprocess.CalledProcessError(1, ['probe'], output='{"error":"covered"}', stderr='detail')
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / 'app.log'
            with patch.object(comparison.preflight, 'dialogs', return_value=[]), \
                    patch.object(comparison, 'stage', side_effect=lambda p: Path('/tmp') / Path(p).name), \
                    patch.object(comparison.subprocess, 'Popen', return_value=process), \
                    patch.object(comparison, 'run', side_effect=error):
                with self.assertRaises(SystemExit):
                    comparison.launch(Path('app'), 'scroll', 21, log)
            failure = json.loads(log.with_suffix('.ready-error.json').read_text())
            self.assertEqual(failure['stdout'], error.stdout)
            self.assertEqual(failure['stderr'], error.stderr)
            process.terminate.assert_called_once()
            process.wait.assert_called_once_with(timeout=10)

    def test_a_short_sample_cannot_pass_by_delivering_all_ticks(self):
        row = dict(framework='zgx', exit_code=0, terminated_after_report=False,
                   ran_seconds=21, requested_seconds=21, workload_ticks=1260,
                   mode='scroll', wall_seconds=10.1)
        with patch.object(comparison, 'SAMPLED', 10):
            self.assertTrue(comparison.valid(row, 'lab'))
            row['wall_seconds'] = 9
            self.assertFalse(comparison.valid(row, 'lab'))

    def test_sampler_delay_is_from_readiness_with_lifetime_padding(self):
        process = Mock(pid=42, benchmark_started=0, benchmark_ready_seconds=2)
        sample = {'wall_seconds': 10, 'samples': 100}
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)

            def launch(binary, mode, seconds, log):
                log.write_text('workload_ticks=1260\n')
                self.assertEqual(seconds, 21)
                return process

            with patch.object(comparison, 'REQUESTED', 15), patch.object(comparison, 'WARMUP', 5), \
                    patch.object(comparison, 'SAMPLED', 10), patch.object(comparison, 'launch', side_effect=launch), \
                    patch.object(comparison, 'run', return_value=json.dumps(sample)) as sampler, \
                    patch.object(comparison, 'finish', return_value=(0, False)), \
                    patch.object(comparison.time, 'monotonic', return_value=21):
                row = comparison.one_trial('lab', 'zgx', 'scroll', 0, out, False)
            sampler.assert_called_once_with([str(comparison.SAMPLER), '42', '5', '10', str(comparison.INTERVAL)])
            self.assertEqual(row['requested_seconds'], 21)
            self.assertEqual(row['ran_seconds'], 21)
            self.assertEqual(row['ready_seconds'], 2)


if __name__ == '__main__':
    unittest.main()
