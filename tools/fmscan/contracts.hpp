#pragma once
#include "player_data.hpp"
#include <tuple>

// All offsets use NativePlayerRecord's attribute view, not ACTUAL_PLAYER's base.
// Discovery deliberately does not require a valid contract: free agents and
// malformed contracts must remain inspectable without trusting their payloads.
inline NativeEntity contract_player(const Memory& m, const NativeCatalog& catalog, const Args& a) {
    U index = number(a.pos[2]), uid = number(a.pos[3]);
    if (index > INT32_MAX || !uid || uid > UINT32_MAX) throw std::runtime_error("invalid player identity");
    auto validate = [&](U record) -> std::optional<NativeEntity> {
        if (record < 0x148) return {};
        auto e = read_entity(m, catalog, record - 0x148, "player");
        if (!e || e->index != index || e->uid != uid) return {};
        return e;
    };
    if (a.has("--record")) {
        auto e = validate(number(a.get("--record"), 16));
        if (!e) throw std::runtime_error("record does not match native RTTI/player identity");
        return *e;
    }
    std::vector<NativeEntity> found;
    auto stats = scan(m, Near(a), [&](U base, const uint8_t* bytes, size_t n) {
        for (size_t i = 0; i + 8 <= n; i += 8) {
            if (stopped) throw std::runtime_error("interrupted");
            if (word(bytes + i) != (uid << 32 | index) || base + i < 0x148) continue;
            auto e = validate(base + i - 0x148);
            if (e) found.push_back(*e);
        }
    });
    require_scan(stats);
    if (found.size() != 1) throw std::runtime_error("expected one native player, found " + std::to_string(found.size()));
    return found.front();
}
inline Json packed_contract_date(uint32_t raw, bool semantic) {
    Json j; j.addr("raw", raw);
    unsigned year = raw >> 16, day = raw & 0x1ff;
    bool leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    bool valid = year >= 2000 && year <= 2200 && day && day <= (leap ? 366u : 365u);
    j.str("status", valid ? (semantic ? "decoded_contract_expiry" : "date_shape_only") : "invalid_or_sentinel");
    if (valid) { j.num("year", year); j.num("day_of_year", day); }
    return j;
}
inline Json contract_team(const Memory& m, const NativeCatalog& catalog, U address) {
    Json j; j.addr("address", address);
    auto t = read_entity(m, catalog, address, "team");
    if (!t) { j.str("status", address ? "invalid_or_unmapped_team" : "absent"); return j; }
    j.str("status", "rtti_and_duplicate_uid_validated"); j.raw("team", entity_json(*t).done());
    U club = 0;
    auto c = m.read(address + 0x30, club) ? read_entity(m, catalog, club, "club") : std::nullopt;
    j.raw("club", c ? entity_json(*c).done() : "null");
    return j;
}
inline Json squad_membership(const Memory& m, const NativeCatalog& catalog, U team, const NativeEntity& player) {
    Json j; j.addr("team_address", team); j.num("vector_offset", 0x38);
    auto entity = read_entity(m, catalog, team, "team");
    if (!entity) { j.str("status", "unavailable_invalid_team"); return j; }
    std::array<U, 3> v{};
    if (!m.read(team + 0x38, v) || v[1] < v[0] || v[2] < v[1] ||
        (v[1] - v[0]) % 8 || (v[2] - v[0]) % 8 || v[2] - v[0] > 4096 * 8 ||
        (!v[0] && (v[1] || v[2])) || (v[0] && v[0] % 8)) {
        j.str("status", "invalid_vector_candidate"); return j;
    }
    U size = v[1] - v[0];
    std::vector<U> entries(size / 8);
    if (size && (!m.region(v[0], size) || !m.read(v[0], entries.data(), size))) {
        j.str("status", "unreadable_vector_candidate"); return j;
    }
    std::vector<std::string> matches;
    for (size_t i = 0; i < entries.size(); ++i)
        if (entries[i] == player.address) matches.push_back(quote(hex(v[0] + i * 8)));
    std::array<U, 3> check{};
    auto fresh = read_entity(m, catalog, team, "team");
    if (!m.read(team + 0x38, check) || check != v || !fresh || fresh->uid != entity->uid || fresh->index != entity->index)
        throw std::runtime_error("squad identity/vector changed during capture; retry");
    j.str("status", "bounded_pointer_vector_candidate"); j.addr("begin", v[0]); j.addr("end", v[1]); j.addr("capacity", v[2]);
    j.num("entry_count", entries.size());
    j.num("capacity_count", (v[2] - v[0]) / 8);
    j.num("spare_slots", (v[2] - v[1]) / 8);
    j.raw("selected_player_slots", array(matches));
    j.str("interpretation", "exact ACTUAL_PLAYER pointer matches only; roster semantics and writes not authorized");
    return j;
}
inline Json inspect_parent_contract(const Memory& m, const NativeCatalog& catalog, const NativeEntity& p, U address) {
    Json j; j.addr("address", address);
    if (!address) { j.str("status", "absent"); return j; }
    std::array<uint8_t, 0x60> bytes{};
    auto type = object_type(m, address);
    if (!m.read(address, bytes)) { j.str("status", "unreadable"); return j; }
    j.raw("rtti", type_json(type).done());
    if (type.name != ".?AVFULL_CONTRACT@db@@" || !native_vtable(m, field<U>(bytes, 0)) ||
        field<U>(bytes, 8) != p.record + 0x140) {
        j.str("status", "type_or_person_owner_mismatch"); return j;
    }
    j.str("status", "rtti_and_person_back_pointer_validated");
    j.raw("parent_team", contract_team(m, catalog, field<U>(bytes, 0x10)).done());
    j.num("weekly_wage_gbp", field<uint32_t>(bytes, 0x20));
    j.raw("expiry", packed_contract_date(field<uint32_t>(bytes, 0x48), true).done());
    j.raw("squad_number", bytes[0x5d] == 255 ? "null" : std::to_string(bytes[0x5d]));
    // Upstream FM26 research lead; never used to enable an editor/write path.
    Json flags; flags.num("offset", 0x57); flags.num("raw", bytes[0x57]);
    flags.str("status", "upstream_layout_candidate_not_locally_display_verified");
    j.raw("transfer_status_candidate", flags.done());
    j.str("scope", "parent_contract_only; loan_destination_not_inferred");
    return j;
}
// Bounded evidence dump. RTTI and team identities are checked; finding a date
// shape or a team pointer here does not establish the object's business meaning.
inline Json transfer_object(const Memory& m, const NativeCatalog& catalog, U address, U person) {
    Json j; j.addr("address", address);
    if (!address) { j.str("status", "absent"); return j; }
    std::array<uint8_t, 0x100> b{};
    auto r = m.region(address, b.size());
    if (!r || !r->heap() || !m.read(address, b)) { j.str("status", "unreadable_or_not_heap"); return j; }
    j.str("status", "raw_candidate_only"); j.raw("rtti", type_json(object_type(m, address)).done());
    j.str("scope", "fixed_256_byte_window; allocation_extent_unknown; may_include_adjacent_objects");
    j.str("person_at_8_status", field<U>(b, 8) == person ? "matches_selected_person" : "does_not_match");
    std::ostringstream raw;
    for (uint8_t byte : b) raw << std::hex << std::setw(2) << std::setfill('0') << unsigned(byte);
    j.str("bytes", raw.str());
    std::vector<std::string> dates, teams;
    for (size_t off = 0; off + 4 <= b.size(); off += 4) {
        auto d = packed_contract_date(field<uint32_t>(b, off), false);
        unsigned year = field<uint32_t>(b, off) >> 16, day = field<uint32_t>(b, off) & 0x1ff;
        bool leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        if (year < 2000 || year > 2200 || !day || day > (leap ? 366u : 365u)) continue;
        d.num("offset", off); dates.push_back(d.done());
    }
    for (size_t off = 0; off + 8 <= b.size(); off += 8) {
        U pointer = field<U>(b, off);
        if (!read_entity(m, catalog, pointer, "team")) continue;
        auto t = contract_team(m, catalog, pointer); t.num("offset", off); teams.push_back(t.done());
    }
    j.raw("date_candidates", array(dates)); j.raw("team_pointer_candidates", array(teams));
    return j;
}
struct ContractCapture {
    Json json;
    std::map<U, std::string> targets;
};
inline ContractCapture capture_contract(const Memory& m, const NativeCatalog& catalog, const NativeEntity& p, bool transfer) {
    auto fresh = read_entity(m, catalog, p.address, "player");
    if (!fresh || fresh->index != p.index || fresh->uid != p.uid) throw std::runtime_error("player identity changed; rediscover");
    std::array<U, 3> slots{}; // agreement, parent contract, loan node
    if (!m.read(p.record + 0x1e0, slots)) throw std::runtime_error("unreadable player contract slots");
    ContractCapture result;
    auto& j = result.json;
    j.raw("player", entity_json(*fresh).done());
    j.raw("contract", inspect_parent_contract(m, catalog, p, slots[1]).done());
    result.targets[p.record + 0x140] = "selected_person";
    if (slots[1]) result.targets[slots[1]] = "parent_contract";
    U loan_object = 0;
    bool loan_read = !slots[2] || m.read(slots[2], loan_object);
    U current_team = 0;
    if (transfer) {
        // Upstream plao.Pctm is ACTUAL_PLAYER+0x130, not attribute-record+0x130.
        if (!m.read(p.address + 0x130, current_team)) throw std::runtime_error("unreadable current-team candidate slot");
        j.raw("current_team_candidate", contract_team(m, catalog, current_team).done());
        j.raw("current_squad_candidate", squad_membership(m, catalog, current_team, p).done());
        U parent_team = 0, owner = 0;
        if (slots[1] && object_type(m, slots[1]).name == ".?AVFULL_CONTRACT@db@@" &&
            m.read(slots[1] + 8, owner) && owner == p.record + 0x140 && m.read(slots[1] + 0x10, parent_team))
            j.raw("parent_squad_candidate", squad_membership(m, catalog, parent_team, p).done());
        j.raw("parent_contract_evidence", transfer_object(m, catalog, slots[1], p.record + 0x140).done());
        std::vector<std::string> links;
        for (auto [off, address, label] : {std::tuple<U,U,const char*>{0x1e0, slots[0], "agreement_slot"},
             {0x1f0, slots[2], "loan_node_slot"}, {0, loan_object, "loan_node_first_pointer"}}) {
            Json link; link.str("relation", label); link.num("offset", off);
            link.raw("object", transfer_object(m, catalog, address, p.record + 0x140).done());
            if (std::string(label) == "loan_node_first_pointer" && !loan_read) link.str("status", "unreadable_node_pointer");
            links.push_back(link.done());
            if (address) result.targets[address] = label;
        }
        j.raw("transfer_candidates", array(links));
        j.str("interpretation", "slots and raw candidates only; presence does not prove active/pending/completed transfer");
    }
    // Avoid publishing a mixture across pointer replacement or person reuse.
    std::array<U, 3> check{}; U check_loan = 0;
    U check_team = 0;
    fresh = read_entity(m, catalog, p.address, "player");
    if (!fresh || fresh->index != p.index || fresh->uid != p.uid ||
        !m.read(p.record + 0x1e0, check) || check != slots ||
        (transfer && (!m.read(p.address + 0x130, check_team) || check_team != current_team)) ||
        (loan_read && slots[2] && (!m.read(slots[2], check_loan) || check_loan != loan_object)))
        throw std::runtime_error("player/contract pointers changed during capture; retry");
    return result;
}
inline int cmd_contract(const Memory& m, const Args& a) {
    const bool watching = a.pos[0] == "transfer-watch", transfer = a.pos[0] != "contract";
    if (watching) a.allow({"--record", "--near", "--ms", "--count"}, 4);
    else if (transfer) a.allow({"--record", "--near", "--refs", "--max"}, 4);
    else a.allow({"--record", "--near"}, 4);
    U max = a.val("--max", 100), ms = a.val("--ms", 250), count = a.val("--count", 0);
    if (max > 4096 || !ms || ms > 60000 || count > 1000000) throw std::runtime_error("max 0..4096, ms 1..60000, count 0..1000000 required");
    NativeCatalog catalog(m);
    auto p = contract_player(m, catalog, a);
    auto capture = capture_contract(m, catalog, p, transfer);
    const auto started = std::chrono::steady_clock::now();
    auto output = [&](const ContractCapture& c, const std::string& event) {
        Json j; j.str("command", a.pos[0]); j.num("pid", m.pid); j.raw("read_only", "true");
        j.raw("requires_open_screen", "false"); j.raw("snapshot", c.json.done());
        j.str("consistency", "sequential_reads_with_identity_and_pointer_recheck; not_atomic");
        if (!event.empty()) { j.str("event", event); j.num("elapsed_ms", elapsed_ns(started) / 1000000); }
        emit(a, j, j.done());
    };
    if (!watching && a.has("--refs")) {
        std::vector<std::string> refs; U total = 0;
        auto stats = scan(m, Near(a), [&](U base, const uint8_t* b, size_t n) {
            for (size_t i = 0; i + 8 <= n; i += 8) {
                if (stopped) throw std::runtime_error("interrupted");
                auto target = capture.targets.find(word(b + i));
                if (target == capture.targets.end()) continue;
                ++total;
                if (refs.size() >= max) continue;
                Json ref; ref.addr("slot", base + i); ref.addr("target", target->first); ref.str("relation", target->second);
                auto owner = nearest_owner(m, base + i);
                ref.addr("nearest_object", owner.addr); ref.raw("rtti", type_json(owner.type).done());
                ref.str("ownership_status", "nearest_preceding_rtti_heuristic_only"); refs.push_back(ref.done());
            }
        });
        require_scan(stats);
        // Recheck roots after the potentially long reference scan.
        auto after = capture_contract(m, catalog, p, transfer);
        if (after.targets != capture.targets) throw std::runtime_error("transfer roots changed during reference scan; retry");
        Json evidence; evidence.raw("references", array(refs)); evidence.num("total", total);
        evidence.raw("truncated", total > refs.size() ? "true" : "false");
        evidence.num("scanned_bytes", stats.bytes); evidence.num("unreadable_chunks", stats.unreadable);
        capture.json.raw("reference_scan", evidence.done());
    }
    output(capture, watching ? "ready" : "");
    if (!watching) return 0;
    std::string previous = capture.json.done();
    for (U poll = 0; !stopped && (!count || poll < count); ++poll) {
        pause_ms(ms); if (stopped) break;
        Memory latest(m.pid); // Include mappings created since the initial scan.
        auto next = capture_contract(latest, catalog, p, true);
        auto current = next.json.done();
        if (current != previous) { output(next, "change"); previous = current; }
    }
    return 0;
}
