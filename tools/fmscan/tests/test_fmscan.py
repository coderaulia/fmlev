"""Real process_vm_readv tests; no FM26 process or writes by the scanner."""
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class ScannerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        cls.folder = Path(cls.tmp.name)
        cls.binary = cls.folder / "fmscan"
        cls.fixture = cls.folder / "fixture"
        for source, output in [(ROOT / "fmscan.cpp", cls.binary),
                               (ROOT / "tests/fixture.cpp", cls.fixture)]:
            subprocess.run(["g++", "-std=c++20", "-O2", "-Wall", "-Wextra",
                            "-Wpedantic", "-Werror", str(source), "-o", str(output)], check=True)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def setUp(self):
        self.target = subprocess.Popen([str(self.fixture), str(self.folder / "game_plugin.dll")],
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        pid, obj, vt = self.target.stdout.readline().split()
        self.pid, self.obj, self.vt = pid, int(obj, 16), vt
        self.near = ["--near", f"{self.obj + 0x800:x}:800"]

    def tearDown(self):
        self.target.communicate("quit\n", timeout=5)

    def action(self, name="action"):
        self.target.stdin.write(name + "\n")
        self.target.stdin.flush()
        self.assertEqual(self.target.stdout.readline().strip(), "ok")

    def run_scan(self, command, *args, near=False, ok=True):
        argv = [str(self.binary), command, self.pid, *map(str, args), "--json"]
        if near:
            argv.extend(self.near)
        result = subprocess.run(argv, capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode == 0, ok, result.stderr)
        return json.loads(result.stdout if ok else result.stderr)

    def test_struct_annotations_and_cycle(self):
        data = self.run_scan("struct", hex(self.obj), "--depth", 8)
        self.assertEqual(len(data["objects"]), 2)
        fields = data["objects"][0]["fields"]
        self.assertEqual(fields[1]["kind"], "heap_pointer")
        self.assertEqual(fields[2]["kind"], "small_int")
        self.assertEqual(fields[3]["halves"][0]["f32"], 1.5)
        self.assertEqual(fields[4]["halves"][0]["date_year"], 2025)
        limited = self.run_scan("struct", hex(self.obj), "--depth", 8, "--max", 1)
        self.assertTrue(limited["truncated"])

    def test_rtti_from_object_and_vtable(self):
        for addr in [hex(self.obj), self.vt]:
            self.assertEqual(self.run_scan("rtti", addr)["type_name"], ".?AVTEST_OBJECT@@")
        self.run_scan("rtti", hex(self.obj + 0x20), ok=False)

    def test_vector_bounds_and_stride(self):
        addr = hex(self.obj + 0x800)
        data = self.run_scan("vec", addr)
        self.assertIsNone(data["element_count"])
        self.assertEqual(data["size_bytes"], 16)
        self.assertEqual(len(data["elements"]), 2)
        self.assertEqual(self.run_scan("vec", addr, "--stride", 8)["element_count"], 2)
        self.run_scan("vec", addr, "--stride", 7, ok=False)
        self.run_scan("vec", addr, "--stride", 0, ok=False)
        self.run_scan("vec", hex(self.obj + 0xa00), ok=False)
        self.assertEqual(self.run_scan("vec", hex(self.obj + 0xb00))["size_bytes"], 0)

    def test_graph_search_and_dump(self):
        graph = self.run_scan("graph", hex(self.obj), "--in", "--out", near=True)
        self.assertEqual(graph["inbound_total"], 2)
        self.assertGreaterEqual(graph["outbound_total"], 2)
        refs = self.run_scan("refs", hex(self.obj), near=True)
        self.assertEqual(refs["total"], 2)
        for kind, value, addr in [("u32", "42", self.obj + 16), ("u64", "42", self.obj + 16),
                                  ("i32", "42", self.obj + 16), ("f32", "1.5", self.obj + 24)]:
            found = self.run_scan("find", kind, value, near=True)
            self.assertIn(hex(addr), found["addresses"])
        dump = self.run_scan("dump", hex(self.obj + 16), 4)
        self.assertEqual(dump["bytes"], "2a000000")
        self.run_scan("graph", hex(self.obj), ok=False)

    def test_snapshot_diff_filters_and_corruption(self):
        snap, noise = self.folder / "snap", self.folder / "noise"
        self.run_scan("snap", snap, near=True)
        self.action()
        delta = self.run_scan("diff", snap, "--to32", "1", "--save-changed", noise)
        self.assertEqual(delta["changed_words"], 1)
        self.assertEqual(delta["changes"][0]["address"], hex(self.obj + 0x30))
        self.assertEqual(self.run_scan("diff", snap, "--to", "1", "--from", "0")["changed_words"], 1)
        self.assertEqual(self.run_scan("diff", snap, "--exclude", noise, "--to", "1")["changed_words"], 0)
        self.assertEqual(self.run_scan("diff", snap, "--max", 0)["changes"], [])
        bad = self.folder / "bad"
        bad.write_bytes(b"short")
        self.run_scan("diff", bad, ok=False)
        self.run_scan("diff", snap, "--save-changed", snap, ok=False)
        alias = self.folder / "snap-alias"
        alias.unlink(missing_ok=True)
        alias.hardlink_to(snap)
        self.run_scan("diff", snap, "--save-changed", alias, ok=False)
        self.run_scan("diff", snap, "--to32", "100000000", ok=False)
        self.run_scan("diff", snap, "--exclude", self.folder / "missing", ok=False)

    def test_watch_changes_partial_word(self):
        watcher = subprocess.Popen([str(self.binary), "watch", self.pid, hex(self.obj + 0x40),
                                    "1", "--ms", "20", "--count", "10", "--json"],
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            self.assertEqual(json.loads(watcher.stdout.readline())["event"], "ready")
            self.action("tail")
            out, err = watcher.communicate(timeout=5)
            self.assertEqual(watcher.returncode, 0, err)
            changes = [json.loads(line) for line in out.splitlines()]
            self.assertEqual(len(changes), 1)
            self.assertEqual(changes[0]["width"], 1)
            self.assertEqual(changes[0]["new"], "0x1")
        finally:
            if watcher.poll() is None:
                watcher.kill()
                watcher.communicate()

    def test_diffset_noise_exclusion_and_ranking(self):
        proc = subprocess.Popen([str(self.binary), "diffset", self.pid, "--noise-ms", "150",
                                 "--action-wait", "1", "--json", *self.near],
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            self.assertIn("Sampling", proc.stderr.readline())
            self.assertIn("Perform action", proc.stderr.readline())
            self.action()
            out, err = proc.communicate(timeout=5)
            self.assertEqual(proc.returncode, 0, err)
            data = json.loads(out)
            self.assertEqual(data["changed_words"], 1)
            self.assertEqual(data["changes"][0]["address"], hex(self.obj + 0x30))
            self.assertEqual(data["changes"][0]["nearest_object"], hex(self.obj))
            self.assertEqual(data["changes"][0]["score"], 110)
        finally:
            if proc.poll() is None:
                proc.kill()
                proc.communicate()

    def test_reject_invalid_input_and_plain_output(self):
        for cmd, args in [("dump", ["garbage", 8]), ("watch", [hex(self.obj), 8, "--ms", 0]),
                          ("struct", ["0", "--depth", 99]), ("dump", ["0", 8]),
                          ("find", ["u32", "4294967296"]), ("find", ["i32", "-2147483649"]),
                          ("refs", ["0", "--bogus", 1])]:
            self.assertIn("error", self.run_scan(cmd, *args, ok=False))
        result = subprocess.run([str(self.binary), "dump", self.pid, hex(self.obj + 16), "4"],
                                capture_output=True, text=True, timeout=5)
        self.assertEqual(result.returncode, 0)
        self.assertIn("2a 00 00 00", result.stdout)
        self.assertEqual(self.run_scan("find", "u32", 42, "--max", 0, near=True)["addresses"], [])

    def test_interrupts_stop_watch_and_clean_diffset(self):
        for command, args in [("watch", [hex(self.obj), "8", "--json"]),
                              ("diffset", ["--noise-ms", "10", *self.near])]:
            proc = subprocess.Popen([str(self.binary), command, self.pid, *args],
                                    stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                    stderr=subprocess.PIPE, text=True,
                                    env={**os.environ, "TMPDIR": str(self.folder)})
            try:
                if command == "watch":
                    self.assertEqual(json.loads(proc.stdout.readline())["event"], "ready")
                else:
                    self.assertIn("Sampling", proc.stderr.readline())
                    self.assertIn("Perform action", proc.stderr.readline())
                files = list(self.folder.glob("fmscan-*"))
                self.assertEqual(len(files), 0 if command == "watch" else 3)
                proc.send_signal(signal.SIGINT)
                proc.communicate(timeout=5)
                self.assertEqual(proc.returncode, 0 if command == "watch" else 1)
                self.assertTrue(all(not p.exists() for p in files))
            finally:
                if proc.poll() is None:
                    proc.kill()
                    proc.communicate()


if __name__ == "__main__":
    unittest.main()
