import json
import re
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class EntityTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        cls.folder = Path(cls.tmp.name)
        cls.binary = cls.folder / "fmscan"
        cls.fixture = cls.folder / "fixture"
        for source, output in [(ROOT / "fmscan.cpp", cls.binary),
                               (ROOT / "tests/entity_fixture.cpp", cls.fixture)]:
            subprocess.run(["g++", "-std=c++20", "-O2", "-Wall", "-Wextra",
                            "-Wpedantic", "-Werror", str(source), "-o", str(output)], check=True)
        subprocess.run(["cargo", "build", "--release", "-p", "fmlev-core", "--example", "player_data"], cwd=ROOT.parents[1], check=True, capture_output=True)
        cls.rust = ROOT.parents[1] / "target/release/examples/player_data"

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def setUp(self):
        self.target = subprocess.Popen([str(self.fixture), str(self.folder / "game_plugin.dll")],
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        self.pid, address = self.target.stdout.readline().split()
        self.root = int(address, 16)

    def tearDown(self):
        self.target.communicate("quit\n", timeout=5)

    def action(self, name):
        self.target.stdin.write(name + "\n")
        self.target.stdin.flush()
        self.assertEqual(self.target.stdout.readline().strip(), "ok")

    def scan(self, kind, *args, ok=True):
        proc = subprocess.run([str(self.binary), "entity", self.pid, kind, *map(str, args),
                               "--near", f"{self.root + 0x8000:x}:8000", "--json"],
                              capture_output=True, text=True, timeout=10)
        self.assertEqual(proc.returncode == 0, ok, proc.stderr)
        return json.loads(proc.stdout if ok else proc.stderr)

    def test_player_and_all_related_entities_without_ui(self):
        data = self.scan("player", "--name", "lamine yamal", "--related")
        self.assertFalse(data["requires_open_screen"])
        self.assertEqual(data["source"], "native_database_memory")
        self.assertEqual(data["validated_records"], 1)
        self.assertEqual(data["total_matches"], 1)
        player = data["matches"][0]
        self.assertEqual(player["name"], "Lamine Yamal Nasraoui")
        self.assertEqual(player["index"], 40629)
        self.assertEqual(player["uid"], 2000256231)
        self.assertEqual(player["record"], hex(self.root + 0x148))
        related = player["related"]
        self.assertEqual({e["name"] for e in related["entities"] if e["kind"] != "team"},
                         {"Barcelona", "Spain", "Spanish First Division"})
        self.assertEqual(len(related["edges"]), 6)
        self.assertEqual(related["unavailable"], [])

    def test_other_entity_searches_and_club_links(self):
        for kind, name, index, uid in [("club", "Barcelona", 1074, 1708),
                                      ("nation", "Spain", 170, 796),
                                      ("competition", "Spanish First Division", 51, 67)]:
            data = self.scan(kind, "--name", name, "--id", index, "--uid", uid, "--related")
            self.assertEqual(data["total_matches"], 1)
            self.assertEqual(data["matches"][0]["name"], name)
            self.assertEqual(data["matches"][0]["related"]["unavailable"], [])
        club = self.scan("club", "--name", "Barcelona", "--related")["matches"][0]
        self.assertIn("Spanish First Division", [e["name"] for e in club["related"]["entities"]])
        self.assertEqual(self.scan("player", "--name", "no such player")["matches"], [])
        self.assertEqual(self.scan("player", "--id", 123)["total_matches"], 0)

    def test_broken_relationships_are_reported(self):
        self.action("bad-contract")
        player = self.scan("player", "--name", "Lamine", "--related")["matches"][0]
        self.assertEqual(player["related"]["entities"], [])
        self.assertEqual(player["related"]["unavailable"][0]["relation"], "parent_contract")

    def test_wrong_type_is_not_a_nation(self):
        self.action("bad-nation")
        player = self.scan("player", "--name", "Lamine", "--related")["matches"][0]
        self.assertTrue(any(e["relation"] == "club_nation" for e in player["related"]["unavailable"]))

    def test_team_vector_requires_owner_back_pointer(self):
        self.action("bad-vector-owner")
        club = self.scan("club", "--id", 1074, "--related")["matches"][0]
        self.assertTrue(any(e["relation"] == "club_first_team_owner" for e in club["related"]["unavailable"]))
        self.assertNotIn("competition", [e["kind"] for e in club["related"]["entities"]])

    def test_duplicate_identity_fails(self):
        self.action("duplicate")
        self.assertIn("ambiguous", self.scan("player", "--name", "Lamine", "--max", 1, ok=False)["error"])

    def test_invalid_utf8_is_not_published(self):
        self.action("bad-name")
        self.assertEqual(self.scan("player")["validated_records"], 0)

    def test_bad_selectors_fail(self):
        for kind, args in [("unknown", []), ("player", ["--max", 0]),
                           ("nation", ["--uid", 4294967296]), ("club", ["--name", ""])]:
            self.assertIn("error", self.scan(kind, *args, ok=False))

    def test_editor_prints_sixty_attributes_and_unknowns(self):
        p = self.scan("player", "--name", "Lamine", "--editor")["matches"][0]["editor_data"]
        self.assertEqual(len(p["attributes"]), 60)
        self.assertEqual(set(p["attributes"].values()), {10})
        self.assertEqual((p["current_ability"], p["potential_ability"]), (170, 195))
        self.assertEqual(p["positions"]["left_foot"], 20)
        self.assertEqual(p["contract"]["wage_per_week"], 280867)
        self.assertEqual(p["contract"]["expiry_year"], 2031)
        self.assertEqual((p["nationality"], p["club_name"]), ("Spain", "Barcelona"))
        for k in ["age", "morale", "condition", "sharpness", "is_injured"]:
            self.assertIsNone(p[k])
        self.assertIsNone(p["contract"]["release_clause"])

    def player_data(self, language, located, ok=True):
        argv = [str(self.binary), "player-data"] if language == "cpp" else [str(self.rust)]
        argv += [self.pid, "40629", "2000256231", "--iterations", "3", "--json"]
        if located:
            argv += ["--record", hex(self.root + 0x148)]
        else:
            argv += ["--near", f"{self.root + 0x8000:x}:8000"]
        proc = subprocess.run(argv, capture_output=True, text=True, timeout=10)
        self.assertEqual(proc.returncode == 0, ok, proc.stderr)
        return json.loads(proc.stdout if ok else proc.stderr)

    def test_cpp_rust_full_scan_and_located_read_parity(self):
        expected = None
        for language in ["cpp", "rust"]:
            for located in [False, True]:
                result = self.player_data(language, located)
                if expected is None:
                    expected = result
                self.assertEqual(result["player"], expected["player"])
                self.assertEqual(result["checksum"], expected["checksum"])
                self.assertEqual(result["iterations"], 3)
                self.assertEqual(result["scan_ns"] == 0, located)
                self.assertEqual(result["scanned_bytes"] == 0, located)

    def test_both_readers_reject_bad_attributes_and_dates(self):
        self.action("bad-attribute")
        for language in ["cpp", "rust"]:
            self.player_data(language, True, ok=False)
        self.action("good-attribute")
        self.action("bad-date")
        for language in ["cpp", "rust"]:
            self.player_data(language, True, ok=False)

    def test_cpp_attribute_schema_matches_production_rust(self):
        source = (ROOT.parents[1] / "crates/fmlev-core/src/native_player.rs").read_text()
        fields = {}
        for group in ["NATIVE_OUTFIELD_ATTRIBUTES", "NATIVE_EXTRA_ATTRIBUTES"]:
            body = re.search(r"attributes!\s*\{\s*" + group + r";(.*?)\n\}", source, re.S)[1]
            fields.update({k: (int(o, 16), int(s)) for k, o, s in re.findall(r"(\w+):\s*(0x[0-9a-f]+)\s*=>\s*(\d+)", body)})
        cpp = (ROOT / "player_data.hpp").read_text()
        other = {k: (int(o, 16), int(s)) for k, o, s in re.findall(r'\{"(\w+)",\s*(0x[0-9a-f]+),\s*(\d+)\}', cpp)}
        self.assertEqual(fields, other)
        self.assertEqual(len(other), 60)


if __name__ == "__main__":
    unittest.main()
