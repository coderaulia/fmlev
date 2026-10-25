"""Contract/transfer discovery against a synthetic process, never the game."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class ContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        cls.folder = Path(cls.tmp.name)
        cls.binary, cls.fixture = cls.folder / "fmscan", cls.folder / "fixture"
        for source, output in [(ROOT / "fmscan.cpp", cls.binary),
                               (ROOT / "tests/contract_fixture.cpp", cls.fixture)]:
            subprocess.run(["g++", "-std=c++20", "-O2", "-Wall", "-Wextra",
                            "-Wpedantic", "-Werror", str(source), "-o", str(output)], check=True)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def setUp(self):
        self.target = subprocess.Popen([str(self.fixture), str(self.folder / "game_plugin.dll")],
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        self.pid, address = self.target.stdout.readline().split()
        self.root = int(address, 16)
        self.identity = [self.pid, "40629", "2000256231"]
        self.near = ["--near", f"{self.root + 0x8000:x}:8000"]

    def tearDown(self):
        self.target.communicate("quit\n", timeout=5)

    def action(self, name):
        self.target.stdin.write(name + "\n")
        self.target.stdin.flush()
        self.assertEqual(self.target.stdout.readline().strip(), "ok")

    def scan(self, command="contract", *args, ok=True):
        result = subprocess.run([str(self.binary), command, *self.identity, *map(str, args),
                                 *self.near, "--json"], capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode == 0, ok, result.stderr)
        return json.loads(result.stdout if ok else result.stderr)

    def test_contract_with_and_without_known_record(self):
        data = self.scan()
        self.assertTrue(data["read_only"])
        self.assertFalse(data["requires_open_screen"])
        self.assertEqual(data["snapshot"], self.scan("contract", "--record", hex(self.root + 0x148))["snapshot"])
        c = data["snapshot"]["contract"]
        self.assertEqual(c["weekly_wage_gbp"], 280867)
        self.assertEqual(c["expiry"]["year"], 2031)
        self.assertEqual(c["parent_team"]["club"]["name"], "Barcelona")
        self.assertEqual(c["squad_number"], 10)
        self.assertIn("transfer_status_candidate", c)

    def test_free_agent_is_inspectable(self):
        self.action("free-agent")
        self.assertEqual(self.scan()["snapshot"]["contract"]["status"], "absent")
        self.assertEqual(self.scan("transfer")["snapshot"]["transfer_candidates"][0]["object"]["status"], "absent")

    def test_reject_identity_and_duplicates(self):
        self.scan("contract", "--record", hex(self.root + 0x149), ok=False)
        self.identity[2] = "123"
        self.scan(ok=False)
        self.identity[2] = "2000256231"
        self.action("duplicate")
        self.assertIn("found 2", self.scan(ok=False)["error"])

    def test_invalid_contracts_are_unavailable_not_decoded(self):
        for action in ["bad-contract", "bad-contract-type"]:
            self.action(action)
            c = self.scan()["snapshot"]["contract"]
            self.assertEqual(c["status"], "type_or_person_owner_mismatch")
            self.assertNotIn("weekly_wage_gbp", c)

    def test_invalid_team_and_date_remain_diagnostics(self):
        self.action("bad-contract-team")
        self.action("bad-date")
        c = self.scan()["snapshot"]["contract"]
        self.assertEqual(c["parent_team"]["status"], "invalid_or_unmapped_team")
        self.assertEqual(c["expiry"]["status"], "invalid_or_sentinel")

    def test_agreement_loan_candidates_and_reference_truncation(self):
        self.action("agreement")
        self.action("loan")
        data = self.scan("transfer", "--refs", "--max", 1)["snapshot"]
        objects = data["transfer_candidates"]
        self.assertIn("TRANSFER_AGREEMENT", objects[0]["object"]["rtti"]["type_name"])
        loan = objects[2]["object"]
        self.assertEqual(loan["person_at_8_status"], "matches_selected_person")
        self.assertEqual(loan["date_candidates"][0]["year"], 2025)
        self.assertEqual(loan["date_candidates"][0]["status"], "date_shape_only")
        self.assertEqual(loan["team_pointer_candidates"][0]["club"]["name"], "Barcelona")
        self.assertTrue(data["reference_scan"]["truncated"])
        self.assertGreater(data["reference_scan"]["total"], 1)
        self.assertEqual(len(data["reference_scan"]["references"]), 1)
        self.assertEqual(self.scan("transfer", "--refs", "--max", 0)["snapshot"]["reference_scan"]["references"], [])

    def test_unreadable_loan_node_and_upstream_flags(self):
        self.action("bad-transfer")
        self.action("status")
        data = self.scan("transfer")["snapshot"]
        self.assertEqual(data["transfer_candidates"][1]["object"]["status"], "unreadable_or_not_heap")
        self.assertEqual(data["transfer_candidates"][2]["status"], "unreadable_node_pointer")
        self.assertEqual(data["contract"]["transfer_status_candidate"]["raw"], 3)

    def test_squad_membership_and_invalid_candidates(self):
        data = self.scan("transfer")["snapshot"]
        self.assertEqual(data["current_team_candidate"]["club"]["name"], "Barcelona")
        self.assertEqual(data["current_squad_candidate"]["entry_count"], 1)
        self.assertEqual(data["current_squad_candidate"]["selected_player_slots"], [hex(self.root + 0xb200)])
        self.action("remove-squad")
        self.assertEqual(self.scan("transfer")["snapshot"]["current_squad_candidate"]["selected_player_slots"], [])
        self.action("bad-squad")
        self.assertEqual(self.scan("transfer")["snapshot"]["current_squad_candidate"]["status"], "invalid_vector_candidate")
        self.action("bad-current-team")
        self.assertEqual(self.scan("transfer")["snapshot"]["current_squad_candidate"]["status"], "unavailable_invalid_team")

    def test_watcher_captures_loan_attachment_and_wage(self):
        watcher = subprocess.Popen([str(self.binary), "transfer-watch", *self.identity,
                                    "--record", hex(self.root + 0x148), "--count", "20",
                                    "--ms", "20", "--json"], stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, text=True)
        try:
            self.assertEqual(json.loads(watcher.stdout.readline())["event"], "ready")
            self.action("loan")
            self.action("wage")
            out, err = watcher.communicate(timeout=5)
            self.assertEqual(watcher.returncode, 0, err)
            events = [json.loads(line) for line in out.splitlines()]
            self.assertTrue(events)
            self.assertTrue(all(e["event"] == "change" for e in events))
            self.assertEqual(events[-1]["snapshot"]["contract"]["weekly_wage_gbp"], 280868)
            self.assertEqual(events[-1]["snapshot"]["transfer_candidates"][2]["object"]["person_at_8_status"], "matches_selected_person")
        finally:
            if watcher.poll() is None:
                watcher.kill()
                watcher.communicate()

    def test_options_are_validated(self):
        for command, args in [("contract", ["--refs"]), ("transfer", ["--max", 4097]),
                              ("transfer-watch", ["--ms", 0]), ("transfer-watch", ["--refs"])]:
            self.scan(command, *args, ok=False)


if __name__ == "__main__":
    unittest.main()
