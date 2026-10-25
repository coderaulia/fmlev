import json
from pathlib import Path
import subprocess
import tempfile
import unittest

HERE = Path(__file__).resolve().parents[1]


class WriteTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        cls.folder = Path(cls.tmp.name)
        cls.writer, cls.reader, cls.fixture, cls.faults = [cls.folder / n for n in ["fmwrite", "fmscan", "fixture", "faults"]]
        for source, dest in [(HERE / "fmwrite.cpp", cls.writer), (HERE / "fmscan.cpp", cls.reader),
                             (HERE / "tests/entity_fixture.cpp", cls.fixture), (HERE / "tests/write_faults.cpp", cls.faults)]:
            subprocess.run(["g++", "-O2", "-std=c++20", "-Wall", "-Wextra", "-Wpedantic", "-Werror",
                            str(source), "-o", str(dest)], check=True)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def setUp(self):
        self.target = subprocess.Popen([str(self.fixture), str(self.folder / "game_plugin.dll")],
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        self.pid, root = self.target.stdout.readline().split()
        self.root = int(root, 16)
        self.plan = self.folder / "transaction.plan"
        self.plan.unlink(missing_ok=True)

    def tearDown(self):
        self.target.communicate("quit\n", timeout=5)

    def action(self, name):
        self.target.stdin.write(name + "\n")
        self.target.stdin.flush()
        self.assertEqual(self.target.stdout.readline().strip(), "ok")

    def call(self, *args, ok=True):
        p = subprocess.run([str(self.writer), *map(str, args), "--json"], capture_output=True, text=True, timeout=10)
        self.assertEqual(p.returncode == 0, ok, p.stderr)
        return json.loads(p.stdout if ok else p.stderr)

    def prepare(self, kind, index, uid, changes, ok=True):
        return self.call("plan", self.pid, kind, index, uid, "--set", changes, "--plan", self.plan, ok=ok)

    def read(self, kind="club", index=1074):
        p = subprocess.run([str(self.reader), "entity", self.pid, kind, "--id", str(index), "--editor",
                            "--near", f"{self.root + 0x8000:x}:8000", "--json"], capture_output=True, text=True, check=True, timeout=10)
        return json.loads(p.stdout)["matches"][0]["editor_data"]

    def fault(self, mode):
        return subprocess.run([str(self.faults), mode, str(self.plan)], capture_output=True, text=True, timeout=10)

    def test_plan_does_not_write_then_apply_restore_money(self):
        p = self.prepare("club", 1074, 1708, "balance=1000,transfer_budget=222222")
        self.assertEqual(p["state"], "prepared")
        self.assertEqual(self.read()["balance"], -1200)
        self.assertEqual(self.plan.stat().st_mode & 0o777, 0o600)
        applied = self.call("apply", self.plan)
        self.assertEqual(applied["readback_polls"], 3)
        self.assertEqual(self.read()["balance"], 1000)
        self.call("restore", self.plan)
        self.assertEqual(self.read()["balance"], -1200)
        self.assertEqual(self.read()["transfer_budget"], 123456)
        self.assertEqual(self.call("show", self.plan)["state"], "restored")

    def test_experimental_opt_in_and_exact_facility_bytes(self):
        self.prepare("club", 1074, 1708, "training_facilities=18,youth_facilities=19,corporate_facilities=18")
        self.call("apply", self.plan, ok=False)
        self.assertEqual(self.read()["training_facilities"], 19)
        self.call("apply", self.plan, "--experimental")
        d = self.read()
        self.assertEqual((d["training_facilities"], d["youth_facilities"], d["corporate_facilities"]), (18, 19, 18))
        self.assertEqual(d["junior_coaching"], 18)
        self.assertEqual(d["world_reputation"], 7557)
        self.call("restore", self.plan)
        self.assertEqual(self.read()["training_facilities"], 19)

    def test_gender_fields_do_not_cross_containers(self):
        self.prepare("nation", 170, 796, "men.youth_rating=146,men.reputation=9012,economic_factor=18")
        self.call("apply", self.plan, "--experimental")
        d = self.read("nation", 170)
        self.assertEqual(d["men"]["youth_rating"], 146)
        self.assertEqual(d["women"]["youth_rating"], 160)
        self.assertEqual(d["men"]["reputation"], 9012)
        self.call("restore", self.plan)
        self.assertEqual(self.read("nation", 170)["men"]["youth_rating"], 145)

    def test_competition_uses_native_scale(self):
        self.prepare("competition", 51, 67, "reputation=179")
        self.call("apply", self.plan, "--experimental")
        self.assertEqual(self.read("competition", 51)["reputation"], 179)
        self.call("restore", self.plan)
        self.assertEqual(self.read("competition", 51)["reputation"], 178)

    def test_invalid_and_nonwritable_fields_never_make_a_plan(self):
        for changes in ["training_facilities=21", "transfer_budget=-1", "name=1", "stadium_capacity=70000",
                        "wage_used_raw_gbp=5000", "state_of_development_native_code=1", "balance=1,balance=2"]:
            self.prepare("club", 1074, 1708, changes, ok=False)
            self.assertFalse(self.plan.exists())
        self.assertEqual(self.read()["balance"], -1200)

    def test_stale_process_or_altered_plan_is_rejected(self):
        self.prepare("club", 1074, 1708, "balance=1000")
        original = self.plan.read_text()
        lines = original.splitlines()
        parts = lines[1].split()
        parts[2] = "0"
        lines[1] = " ".join(parts)
        self.plan.write_text("\n".join(lines) + "\n")
        self.call("apply", self.plan, ok=False)
        self.plan.write_text(original.replace(hex(self.root + 0xd014), hex(self.root + 0xd018)))
        self.call("apply", self.plan, ok=False)
        self.assertEqual(self.read()["balance"], -1200)

    def test_ownership_change_unknown_build_and_changed_value(self):
        self.prepare("club", 1074, 1708, "training_facilities=18")
        self.action("bad-facility")
        self.call("apply", self.plan, "--experimental", ok=False)
        self.action("unknown-build")
        self.call("apply", self.plan, "--experimental", ok=False)

    def test_failure_on_second_write_rolls_back_first(self):
        self.prepare("club", 1074, 1708, "balance=1000,transfer_budget=222222")
        p = self.fault("fail-second")
        self.assertEqual(p.returncode, 1)
        self.assertIn("rollback verified", p.stderr)
        self.assertEqual(self.read()["balance"], -1200)
        self.assertEqual(self.call("show", self.plan)["state"], "rolled_back")

    def test_partial_write_even_out_of_range_is_restored(self):
        self.action("high-reputation")
        self.prepare("club", 1074, 1708, "reputation=9983")
        p = self.fault("partial-first")
        self.assertEqual(p.returncode, 1)
        self.assertIn("rollback verified", p.stderr)
        self.assertEqual(self.read()["reputation"], 9999)

    def test_failed_rollback_is_not_reported_as_success(self):
        self.prepare("club", 1074, 1708, "balance=1000,transfer_budget=222222")
        p = self.fault("rollback-fail")
        self.assertEqual(p.returncode, 1)
        self.assertIn("ROLLBACK NOT VERIFIED", p.stderr)
        self.assertEqual(self.call("show", self.plan)["state"], "rollback_failed")
        self.call("restore", self.plan)
        self.assertEqual(self.read()["balance"], -1200)

    def test_crash_journal_recovers_partial_transaction(self):
        self.prepare("club", 1074, 1708, "balance=1000,transfer_budget=222222")
        self.assertEqual(self.fault("crash-first").returncode, 86)
        self.assertEqual(self.call("show", self.plan)["state"], "applying")
        self.call("restore", self.plan)
        self.assertEqual(self.read()["balance"], -1200)

    def test_other_changes_are_not_overwritten_on_rollback(self):
        self.prepare("club", 1074, 1708, "training_facilities=18")
        p = self.fault("change-first")
        self.assertIn("ROLLBACK NOT VERIFIED", p.stderr)
        self.assertEqual(self.read()["training_facilities"], 17)
        self.call("restore", self.plan, ok=False)

    def test_journal_not_overwritten_and_restoration_is_idempotent(self):
        self.prepare("club", 1074, 1708, "balance=-1200")
        self.prepare("club", 1074, 1708, "balance=2000", ok=False)
        d = self.call("apply", self.plan)
        self.assertEqual(d["changed_fields"], 0)
        self.call("apply", self.plan, ok=False)
        self.call("restore", self.plan)
        self.call("restore", self.plan)

    def test_duplicate_or_shared_club_records_refuse_writes(self):
        self.prepare("club", 1074, 1708, "training_facilities=18")
        self.action("shared-club")
        self.assertIn("shared", self.call("apply", self.plan, "--experimental", ok=False)["error"])
        self.assertEqual(self.read()["training_facilities"], 19)
