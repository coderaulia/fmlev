import contextlib
import importlib.util
import io
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("control", Path(__file__).with_name("server_control.py"))
control = importlib.util.module_from_spec(spec)
spec.loader.exec_module(control)


class LifecycleTests(unittest.TestCase):
    def test_foreign_listener_is_never_owned(self):
        with patch.object(control, "listeners", return_value={123}), patch.object(control, "executable", return_value="/usr/bin/other"):
            with self.assertRaisesRegex(RuntimeError, "another program"):
                control.owned_server()

    def test_ambiguous_listeners_are_rejected(self):
        with patch.object(control, "listeners", return_value={123, 456}):
            with self.assertRaisesRegex(RuntimeError, "Multiple listener"):
                control.owned_server()

    def test_old_binary_can_be_controlled(self):
        with patch.object(control.os, "readlink", return_value=str(control.BINARY) + " (deleted)"):
            self.assertEqual(control.executable(123), str(control.BINARY))

    def run_main(self, args):
        with tempfile.TemporaryDirectory() as state, patch.object(control, "STATE", Path(state)), patch("sys.argv", ["control", *args]), contextlib.redirect_stdout(io.StringIO()):
            control.main()

    def test_start_reuses_existing_without_build_or_spawn(self):
        with patch.object(control, "owned_server", return_value=123), patch.object(control, "describe"), patch.object(control, "open_browser"), patch.object(control, "watch_existing") as watch, patch.object(control.subprocess, "run") as build, patch.object(control.subprocess, "Popen") as spawn:
            self.run_main(["start"])
            build.assert_not_called()
            spawn.assert_not_called()
            watch.assert_called_once_with(123)

    def test_listener_exit_during_owner_scan_is_not_reported_as_foreign(self):
        with patch.object(control, "listener_inodes", side_effect=[{"socket:[123]"}, set()]):
            self.assertEqual(control.listeners(), set())

    def test_failed_build_preserves_existing_server(self):
        with patch.object(control, "owned_server", return_value=123), patch.object(control.subprocess, "run", side_effect=subprocess.CalledProcessError(1, "cargo")), patch.object(control, "stop") as stop:
            with self.assertRaises(subprocess.CalledProcessError):
                self.run_main(["restart"])
            stop.assert_not_called()

    def test_stop_uses_validated_listener(self):
        with patch.object(control, "owned_server", return_value=123), patch.object(control, "stop") as stop:
            self.run_main(["stop"])
            stop.assert_called_once_with(123)


if __name__ == "__main__":
    unittest.main()
