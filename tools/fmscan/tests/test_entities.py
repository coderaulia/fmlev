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

    def test_staff_identity_names_and_raw_blocks(self):
        data = self.scan("staff", "--name", "guardiola", "--editor")
        self.assertEqual(data["total_matches"], 1, data)
        staff = data["matches"][0]
        self.assertEqual((staff["name"], staff["index"], staff["uid"]), ("Pep Guardiola", 79, 1120))
        self.assertEqual(staff["address"], hex(self.root + 0xf100))
        self.assertEqual(staff["record"], hex(self.root + 0xf000))
        editor = staff["editor_data"]
        self.assertEqual(editor["write_support"], "none")
        attrs = {a["field"]: a for a in editor["attributes"]}
        self.assertEqual(len(attrs), 22)
        self.assertEqual((attrs["physiotherapy"]["group"], attrs["physiotherapy"]["value"]), ("medical", 10))
        self.assertEqual((attrs["attacking"]["value"], attrs["attacking"]["native"]), (18, 89))
        self.assertEqual(attrs["negotiating"]["value"], 1, "native 1 displays as 1")
        self.assertEqual(attrs["tactical_knowledge"]["value"], 3, "native 16 rounds to 3")
        self.assertEqual(attrs["working_with_youngsters"]["value"], 16)
        self.assertNotIn("native", attrs["authority"])
        self.assertEqual(attrs["adaptability"]["value"], 10)
        self.assertEqual(attrs["determination"]["value"], 16)
        self.action("bad-staff-attribute")
        bad = {a["field"]: a for a in self.scan("staff", "--name", "guardiola", "--editor")["matches"][0]["editor_data"]["attributes"]}
        self.assertIsNone(bad["attacking"]["value"])
        self.assertIn("out of range", bad["attacking"]["reason"])
        block = editor["raw_blocks"][1]
        self.assertEqual(block["bytes"], list(range(10, 18)))
        self.assertEqual(block["status"], "unmapped_raw_bytes_heuristic")
        self.assertEqual(self.scan("staff", "--id", 80)["total_matches"], 0, "copy without +0x128 owner")
        self.scan("staff", "--name", "guardiola", "--related", ok=False)
        self.action("bad-staff-uid")
        self.assertEqual(self.scan("staff", "--name", "guardiola")["total_matches"], 0)

    def test_staff_information_candidates_and_contract_survey(self):
        editor = self.scan("staff", "--name", "guardiola", "--editor")["matches"][0]["editor_data"]
        info = editor["information"]
        self.assertEqual(info["status"], "offsets_from_task_brief_not_cross_checked")
        fields = {f["field"]: f for f in info["fields"]}
        self.assertEqual({k: v["value"] for k, v in fields.items()},
                         {"reputation_0xd4": 5000, "reputation_0xd6": 6000, "reputation_0xd8": 7000,
                          "current_ability": 120, "potential_ability": 160, "birth_year": 1971})
        self.assertTrue(all(f["status"] == "candidate_unverified" for f in fields.values()))
        survey = editor["contract_discovery"]
        links = {l["offset"]: l for l in survey["links"]}
        self.assertEqual(set(links), {0x1e0, 0x1f0}, survey)
        direct = links[0x1e0]
        self.assertEqual((direct["pattern"], direct["rtti"]), ("direct", ".?AVFULL_CONTRACT@db@@"))
        self.assertEqual(direct["person_back_pointer_matches_staff"], True)
        cand = direct["contract_candidate"]
        self.assertEqual(cand["status"], "player_layout_decode_not_verified_for_staff")
        self.assertEqual(cand["weekly_wage_raw_u32"], 45000)
        self.assertEqual((cand["expiry_candidate"]["year"], cand["expiry_candidate"]["day_of_year"]), (2028, 100))
        self.assertEqual(cand["team_pointer_rtti"], ".?AVTEAM@db@@")
        node = links[0x1f0]
        self.assertEqual(node["pattern"], "node_first_qword")
        self.assertEqual(node["person_back_pointer_matches_staff"], False, "contract owned by another person")
        self.assertEqual(survey["status"], "heuristic_pointer_survey_of_first_0x240_bytes")
        self.assertGreaterEqual(survey["unnamed"], 2, "name pooled refs are heap pointers without RTTI")
        self.action("bad-staff-info")
        bad = {f["field"]: f for f in self.scan("staff", "--name", "guardiola", "--editor")["matches"][0]["editor_data"]["information"]["fields"]}
        self.assertIsNone(bad["current_ability"]["value"])
        self.assertIn("out of range", bad["current_ability"]["reason"])
        self.assertEqual(bad["potential_ability"]["value"], 160, "other fields stay independent")

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

    def test_preferred_moves_name_only_verified_bits(self):
        ppm = self.scan("player", "--name", "Lamine", "--editor")["matches"][0]["editor_data"]["preferred_moves"]
        self.assertEqual(ppm["status"], "partial_bit_table")
        self.assertEqual(ppm["offset_from_record"], "0x200")
        self.assertEqual(ppm["bitset"], hex((1 << 39) | (1 << 35) | (1 << 3)))
        self.assertEqual(ppm["popcount"], 3)
        self.assertEqual([b["bit"] for b in ppm["set_bits"]], [3, 35, 39])
        self.assertEqual([b["name"] for b in ppm["set_bits"]], [None, None, "Cuts Inside From Both Wings"])
        self.assertEqual([b["verified"] for b in ppm["set_bits"]], [False, False, True])
        self.assertEqual(ppm["named_moves"], ["Cuts Inside From Both Wings"])
        self.assertEqual(ppm["unnamed_set_bits"], 2)
        self.action("clear-ppm")
        ppm = self.scan("player", "--name", "Lamine", "--editor")["matches"][0]["editor_data"]["preferred_moves"]
        self.assertEqual((ppm["bitset"], ppm["popcount"], ppm["set_bits"]), ("0x0", 0, []))

    def test_preferred_moves_text_marks_unverified_bits(self):
        proc = subprocess.run([str(self.binary), "entity", self.pid, "player", "--name", "Lamine", "--editor",
                               "--near", f"{self.root + 0x8000:x}:8000"], capture_output=True, text=True, timeout=10)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("bit 39: Cuts Inside From Both Wings", proc.stdout)
        self.assertIn("bit 35: unknown (unverified)", proc.stdout)

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

    def details(self, kind, index):
        return self.scan(kind, "--id", index, "--editor")["matches"][0]["editor_data"]

    def test_club_native_details(self):
        d = self.details("club", 1074)
        for key, value in {"training_facilities": 19, "youth_facilities": 20,
                           "junior_coaching": 18, "youth_recruitment": 17,
                           "corporate_facilities": 19, "balance": -1200,
                           "transfer_budget": 123456, "reputation": 9098,
                           "world_reputation": 7557, "transfer_retention_pct": 30,
                           "wage_budget_raw_gbp": 100000}.items():
            self.assertEqual(d[key], value, key)
        self.assertEqual(d["currency"], "GBP")
        self.assertIn("unverified", d["wage_period"])
        self.assertIsNone(d["board_patience"])
        self.assertEqual(d["data_analysis_facilities"], 18)
        self.assertEqual(d["sugar_daddy_type"], 1)

    def test_nation_gender_specific_youth_and_reputation(self):
        d = self.details("nation", 170)
        self.assertEqual(d["three_letter_code"], "ESP")
        self.assertEqual(d["continent"], "Europe")
        self.assertEqual((d["men"]["youth_rating"], d["women"]["youth_rating"]), (145, 160))
        self.assertEqual((d["men"]["reputation"], d["women"]["reputation"]), (9011, 9300))
        self.assertIsNone(d["youth_rating"])
        self.assertEqual(d["economic_factor"], 17)
        self.assertEqual(d["state_of_development_native_code"], 1)
        self.assertEqual(d["population"], 49570725)
        self.assertIsNone(d["state_of_development"])
        self.assertIsNone(d["men"]["fifa_rank"])

    def confidence(self, d):
        return {row["field"]: row["confidence"] for row in d["field_confidence"]}

    def test_nationality_rule_candidates_carry_confidence_labels(self):
        spain, japan = self.details("nation", 170), self.details("nation", 171)
        self.assertEqual((spain["nationality_years"], japan["nationality_years"]), (10, 5))
        self.assertEqual((spain["gain_nationality_type_native_code"], japan["gain_nationality_type_native_code"]), (1, 2))
        self.assertIs(spain["dual_nationality_allowed"], True)
        self.assertIs(japan["dual_nationality_allowed"], False)
        self.assertIsNone(spain["gain_nationality_type"])
        labels = self.confidence(spain)
        self.assertEqual(labels["nationality_years"], "probable")
        for key in ("gain_nationality_type_native_code", "dual_nationality_allowed",
                    "economic_factor", "state_of_development_native_code"):
            self.assertEqual(labels[key], "unverified", key)
        self.assertNotIn("verified", labels.values())
        evidence = {e["field"]: e for e in spain["field_evidence"]}
        self.assertEqual(evidence["nationality_years"]["address"], hex(self.root + 0x41ed))
        self.assertEqual(evidence["dual_nationality_allowed"]["address"], hex(self.root + 0x41e9))
        self.assertIn("Nygn", evidence["nationality_years"]["evidence"])
        self.assertEqual(spain["write_support"], "none")

    def test_nationality_type_outside_seen_range_is_unavailable(self):
        self.action("bad-nation-type")
        d = self.details("nation", 170)
        self.assertIsNone(d["gain_nationality_type_native_code"])
        self.assertTrue(any(u["field"] == "gain_nationality_type_native_code" for u in d["unavailable"]))
        self.assertEqual(d["nationality_years"], 10)

    def test_nationality_rules_require_known_build(self):
        self.action("unknown-build")
        d = self.details("nation", 170)
        for key in ("nationality_years", "gain_nationality_type_native_code", "dual_nationality_allowed"):
            self.assertIsNone(d[key], key)

    def test_nation_rules_report_summarises_distribution_without_claiming_verified(self):
        report = subprocess.run(
            ["python3", str(ROOT / "nation_rules.py"), self.pid, "--fmscan", str(self.binary),
             "--near", f"{self.root + 0x8000:x}:8000", "--json"],
            capture_output=True, text=True, timeout=30)
        self.assertEqual(report.returncode, 0, report.stderr)
        data = json.loads(report.stdout)
        self.assertEqual(data["nations"], 2)
        self.assertEqual(data["distribution"]["years"], {"5": 1, "10": 1})
        self.assertEqual(data["distribution"]["dual_nationality_allowed"], {"false": 1, "true": 1})
        self.assertEqual(data["hints"]["Spain"]["status"], "agree")
        self.assertEqual(data["hints"]["Japan"]["status"], "agree")
        self.assertEqual(data["hints"]["Italy"]["status"], "not_found")
        self.assertNotEqual(data["verdict"]["nationality_years"], "verified")
        self.assertIn("screen", data["verdict"]["note"])

    def test_competition_native_reputation_scale_and_stage(self):
        d = self.details("competition", 51)
        self.assertEqual(d["reputation"], 178)
        self.assertIn("1..200", d["reputation_scale"])
        self.assertEqual(d["stage_registration_gate"], 1)
        self.assertEqual(d["short_name"], "First Division")
        self.assertIsNone(d["max_squad_size"])
        self.assertIsNone(d["max_foreign_players"])
        self.assertIsNone(d["winner_prize_money"])

    def test_new_numeric_layouts_require_known_build(self):
        self.action("unknown-build")
        self.assertIsNone(self.details("club", 1074)["training_facilities"])
        self.assertIsNone(self.details("nation", 170)["men"]["youth_rating"])
        self.assertIsNone(self.details("competition", 51)["reputation"])

    def test_invalid_facility_is_unavailable_not_clamped(self):
        self.action("bad-facility")
        d = self.details("club", 1074)
        self.assertIsNone(d["training_facilities"])
        self.assertEqual(d["youth_facilities"], 20)

    def test_finance_owner_required(self):
        self.action("bad-finance-owner")
        d = self.details("club", 1074)
        self.assertIsNone(d["balance"])
        self.assertIsNone(d["transfer_budget"])
        self.assertIsNone(d["corporate_facilities"])

    def test_national_container_owner_required(self):
        self.action("bad-container-owner")
        d = self.details("nation", 170)
        self.assertIsNone(d["men"]["youth_rating"])
        self.assertIsNone(d["men"]["reputation"])
        self.assertEqual(d["women"]["youth_rating"], 160)

    def test_national_team_back_pointer_required(self):
        self.action("bad-national-team-owner")
        d = self.details("nation", 170)
        self.assertIsNone(d["men"]["reputation"])
        self.assertEqual(d["men"]["youth_rating"], 145)

    def test_youth_rating_read_as_word_and_range_checked(self):
        self.action("bad-youth")
        self.assertIsNone(self.details("nation", 170)["men"]["youth_rating"])

    def test_invalid_stage_does_not_invent_registration_status(self):
        self.action("bad-stage")
        d = self.details("competition", 51)
        self.assertIsNone(d["stage_registration_gate"])
        self.assertEqual(d["reputation"], 178)

    def test_competition_reputation_does_not_use_fixture_scale(self):
        self.action("bad-comp-reputation")
        self.assertIsNone(self.details("competition", 51)["reputation"])

    def test_stadium_is_first_teams_ground_not_training_ground(self):
        d = self.details("club", 1074)
        self.assertEqual(d["stadium_name"], "Camp Nou")
        self.assertEqual(d["training_ground_name"], "Training Ground")
        self.assertEqual(d["stadium_capacity"], 62500)
        self.assertEqual(d["stadium_expansion_cap"], 105000)


if __name__ == "__main__":
    unittest.main()
