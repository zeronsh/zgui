import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from types import SimpleNamespace
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
                    patch.object(comparison.time, 'monotonic', side_effect=[100, 100.3]):
                self.assertIs(comparison.launch(Path('app'), 'scroll', 18, log), process)
            probe.assert_called_once_with([str(comparison.READY), '42', str(comparison.LIFETIME_MARGIN - 0.5)])
            self.assertEqual(spawn.call_args.kwargs['env']['ZGX_SECONDS'], '18')
            self.assertEqual(process.benchmark_started, 100)
            self.assertAlmostEqual(process.benchmark_ready_seconds, 0.3)
            self.assertAlmostEqual(json.loads(log.with_suffix('.ready.json').read_text())['spawn_to_ready_seconds'], 0.3)

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
                with self.assertRaises(comparison.ReadinessFailure):
                    comparison.launch(Path('app'), 'scroll', 18, log)
            failure = json.loads(log.with_suffix('.ready-error.1.json').read_text())
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
        process = Mock(pid=42, benchmark_started=0, benchmark_ready_seconds=0.3)
        sample = {'wall_seconds': 10, 'samples': 100}
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)

            def launch(binary, mode, seconds, log):
                log.write_text('workload_ticks=1080\n')
                self.assertEqual(seconds, 18)
                return process

            with patch.object(comparison, 'REQUESTED', 15), patch.object(comparison, 'WARMUP', 5), \
                    patch.object(comparison, 'SAMPLED', 10), patch.object(comparison, 'launch', side_effect=launch), \
                    patch.object(comparison, 'run', return_value=json.dumps(sample)) as sampler, \
                    patch.object(comparison, 'finish', return_value=(0, False)), \
                    patch.object(comparison.time, 'monotonic', return_value=18):
                row = comparison.one_trial('lab', 'zgx', 'scroll', 0, out, False)
            sampler.assert_called_once_with([str(comparison.SAMPLER), '42', '5', '10', str(comparison.INTERVAL)])
            self.assertEqual(row['requested_seconds'], 18)
            self.assertEqual(row['ran_seconds'], 18)
            self.assertEqual(row['ready_seconds'], 0.3)

    def test_readiness_failure_retries_and_counts_both_launches(self):
        row = dict(exit_code=0, terminated_after_report=False)
        failures = []
        launches = {}
        with patch.object(comparison, 'one_trial', side_effect=[comparison.ReadinessFailure('slow'), row]), \
                patch.object(comparison, 'READINESS_FAILURES', failures), \
                patch.object(comparison, 'LAUNCHES', launches), patch.object(comparison.time, 'sleep'):
            self.assertIs(comparison.trial('lab', 'zgx', 'scroll', 0, Path('/tmp'), False), row)
        self.assertEqual(launches, {'zgx': {'trial': 2}})
        self.assertEqual(len(failures), 1)
        self.assertFalse(failures[0]['exhausted'])

    def test_exhausted_readiness_is_recorded_without_a_fake_sample(self):
        failures = []
        with patch.object(comparison, 'launch', side_effect=comparison.ReadinessFailure('covered')), \
                patch.object(comparison, 'READINESS_FAILURES', failures), \
                patch.object(comparison, 'LAUNCHES', {}), patch.object(comparison.time, 'sleep'):
            with self.assertRaises(comparison.ReadinessFailure):
                comparison.pacing('lab', 'zgx', 'scroll', 0, Path('/tmp'))
        self.assertEqual(len(failures), 2)
        self.assertTrue(failures[-1]['exhausted'])

    def test_system_dialog_is_not_a_retryable_failure(self):
        process = Mock(pid=42)
        process.poll.return_value = None
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / 'app.log'
            with self.assertRaises(SystemExit):
                comparison.readiness_error(process, log, {'error': 'dialog', 'dialogs': ['SecurityAgent']})
            process.terminate.assert_called_once()

    def test_ready_after_lifetime_budget_is_a_slow_launch(self):
        process = Mock(pid=42)
        process.poll.return_value = None
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / 'app.log'
            with patch.object(comparison.preflight, 'dialogs', return_value=[]), \
                    patch.object(comparison, 'stage', side_effect=lambda p: Path('/tmp') / Path(p).name), \
                    patch.object(comparison.subprocess, 'Popen', return_value=process), \
                    patch.object(comparison, 'run', return_value='{"ready":true}'), \
                    patch.object(comparison.time, 'monotonic', side_effect=[100, 103]):
                with self.assertRaises(comparison.ReadinessFailure):
                    comparison.launch(Path('app'), 'scroll', 18, log)
            process.terminate.assert_called_once()

    def test_exhausted_readiness_skips_sample_and_other_frameworks_continue(self):
        failures = []
        called = []
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            binary = out / 'fake-binary'
            binary.write_bytes(b'build proof')
            args = SimpleNamespace(into=None, out=out, scene='lab', quick=False,
                                   pgo=False, no_pacing=True, suite=False, powermetrics=False)

            def trial(scene, framework, mode, repeat, place, power, paced=None):
                called.append(framework)
                if framework == 'zgx':
                    failures.append({'exhausted': True})
                    raise comparison.ReadinessFailure('covered twice')
                row = dict.fromkeys(comparison.METRICS + comparison.POWER, 0)
                row.update(framework=framework, mode=mode, repeat=repeat, samples=100,
                           wall_seconds=15, ran_seconds=22, requested_seconds=22,
                           workload_ticks=1320 if framework != 'desktop' else 0,
                           exit_code=0, terminated_after_report=False)
                return row

            with patch.object(comparison, 'trial', side_effect=trial), \
                    patch.object(comparison, 'READINESS_FAILURES', failures), \
                    patch.object(comparison, 'SCENES', {'lab': {'binaries': {'zgx': binary, 'rune': binary}}}), \
                    patch.object(comparison, 'run', return_value=''), \
                    patch.object(comparison, 'environment', return_value={}), \
                    patch.object(comparison, 'toolchains', return_value={}), \
                    patch.object(comparison, 'source_diff_sha256', return_value=''), \
                    patch.object(comparison.time, 'sleep'), patch('builtins.print'):
                comparison.measure(args, ['zgx', 'rune'], ['scroll'], 1, 'commit', {})
            self.assertEqual(called, ['desktop', 'zgx', 'rune'])
            audit = json.loads((out / 'audit.json').read_text())
            self.assertFalse(audit['accepted'])
            self.assertEqual(audit['readiness_failures'], failures)
            summary = json.loads((out / 'summary.json').read_text())
            missing = next(row for row in summary['groups'] if row['framework'] == 'zgx')
            self.assertEqual(missing['repeats'], 0)


if __name__ == '__main__':
    unittest.main()
