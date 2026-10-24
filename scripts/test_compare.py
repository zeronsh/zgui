"""Sampler failure evidence tests; no GUI or subprocesses are launched."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location("compare", Path(__file__).with_name("compare.py"))
compare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(compare)


class TrialEvidence(unittest.TestCase):
    def trial(self, exit_code=0, text="workload_ticks=42\n", timeout=False):
        with tempfile.TemporaryDirectory(dir="/tmp") as directory:
            output = str(Path(directory) / "comparison.csv")
            proc = Mock(pid=123)
            proc.poll.side_effect = [None, None, None if timeout else exit_code, -9 if timeout else exit_code]
            proc.wait.return_value = -9 if timeout else exit_code
            def launch(*args, **kwargs):
                kwargs["stdout"].write(text)
                return proc
            times = [0., 1., 1., 2., 2., 30., 31.] if timeout else [0., 1., 1., 2., 2., 3.]
            with patch.object(compare.subprocess, "Popen", side_effect=launch), patch.object(compare, "sample", side_effect=[(.1, 100), (.2, 200)]), patch.object(compare.time, "monotonic", side_effect=times), patch.object(compare.time, "sleep"):
                try:
                    row = compare.run_trial(Path("fixture"), "zgx", "stream", 0, 3., .5, output)
                    error = None
                except RuntimeError as caught:
                    row, error = None, caught
            raw = json.loads(Path(output + ".zgx.stream.0.json").read_text())
            self.assertEqual(raw["samples"], [dict(elapsed_seconds=1., cpu_seconds=.1, rss_bytes=100), dict(elapsed_seconds=2., cpu_seconds=.2, rss_bytes=200)])
            self.assertEqual((raw["requested_seconds"], raw["warmup_seconds"]), (3., .5))
            self.assertFalse(Path(output).exists(), "trial helper never emits a CSV row")
            return row, error, raw, proc

    def test_failed_process_preserves_samples(self):
        row, error, raw, _ = self.trial(exit_code=101, text="")
        self.assertIsNone(row)
        self.assertIsNotNone(error)
        self.assertIsNone(raw["summary"])
        self.assertEqual(raw["exit_code"], 101)
        self.assertIn("exit=101", raw["failure_reason"])

    def test_timeout_preserves_samples_and_reaps_child(self):
        _, error, raw, proc = self.trial(timeout=True)
        self.assertIn("did not terminate", str(error))
        self.assertEqual(raw["exit_code"], -9)
        proc.kill.assert_called_once()
        proc.wait.assert_called_once()

    def test_missing_or_incompatible_ticks_preserves_samples(self):
        for text in ["", "workload_ticks=0\n"]:
            with self.subTest(text=text):
                _, error, raw, _ = self.trial(text=text)
                self.assertIsNotNone(error)
                self.assertIsNone(raw["summary"])
                self.assertEqual(raw["exit_code"], 0)
                self.assertIn("ticks", raw["failure_reason"])

    def test_success_keeps_existing_artifact_shape(self):
        row, error, raw, _ = self.trial()
        self.assertIsNone(error)
        self.assertEqual(row, raw["summary"])
        self.assertEqual(row["workload_ticks"], 42)
        self.assertEqual(set(raw), {"summary", "requested_seconds", "warmup_seconds", "process_wall_seconds", "application_reports", "samples"})

    def test_launch_failure_writes_status_without_exit_code(self):
        with tempfile.TemporaryDirectory(dir="/tmp") as directory:
            output = str(Path(directory) / "comparison.csv")
            with patch.object(compare.subprocess, "Popen", side_effect=OSError("launch failed")):
                with self.assertRaisesRegex(OSError, "launch failed"):
                    compare.run_trial(Path("missing"), "zgx", "idle", 0, 3., 1., output)
            raw = json.loads(Path(output + ".zgx.idle.0.json").read_text())
            self.assertIsNone(raw["exit_code"])
            self.assertEqual(raw["samples"], [])
            self.assertIn("launch failed", raw["failure_reason"])

    def test_evidence_write_failure_is_not_hidden(self):
        with tempfile.TemporaryDirectory(dir="/tmp") as directory:
            with patch.object(compare.subprocess, "Popen", side_effect=OSError("launch failed")), patch.object(compare.pathlib.Path, "write_text", side_effect=OSError("disk full")):
                with self.assertRaisesRegex(OSError, "disk full") as caught:
                    compare.run_trial(Path("missing"), "zgx", "idle", 0, 3., 1., str(Path(directory) / "out"))
            self.assertIn("launch failed", str(caught.exception.__context__))


if __name__ == "__main__":
    unittest.main()
