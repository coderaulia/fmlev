// Read-only process memory discovery. Build: g++ -O2 -std=c++20 -o fmscan fmscan.cpp
#include "scan.hpp"
#include "entity_details.hpp"
#include "contracts.hpp"

static const char* HELP = R"(fmscan: read-only FM26 memory discovery (never writes to the target)
Usage:
  fmscan snap PID FILE [--near HEX[:RADIUS]]
  fmscan diff PID FILE [--save-changed FILE] [--exclude FILE]
                      [--near HEX[:RADIUS]] [--from HEX] [--to HEX] [--to32 HEX] [--max N]
  fmscan refs PID HEX_PTR [--near HEX[:RADIUS]] [--max N]
  fmscan find PID i32|u32|u64|f32 VALUE [--near HEX[:RADIUS]] [--max N]
  fmscan dump PID HEX_ADDR LEN
  fmscan struct PID HEX_ADDR [--depth N] [--max OBJECTS]
  fmscan vec PID HEX_ADDR [--stride N] [--max N]
  fmscan rtti PID HEX_VTABLE_OR_OBJECT
  fmscan entity PID player|club|nation|competition [--name TEXT] [--id N] [--uid N]
                    [--related] [--editor] [--max N] [--near HEX[:RADIUS]]
  fmscan player-data PID INDEX UID [--record HEX] [--iterations N] [--near HEX[:RADIUS]]
  fmscan contract PID INDEX UID [--record HEX] [--near HEX[:RADIUS]]
  fmscan transfer PID INDEX UID [--record HEX] [--refs] [--max N] [--near HEX[:RADIUS]]
  fmscan transfer-watch PID INDEX UID [--record HEX] [--near HEX[:RADIUS]] [--ms N] [--count POLLS]
  fmscan graph PID HEX_OBJECT --in|--out [--near HEX[:RADIUS]] [--len N] [--max N]
  fmscan watch PID HEX_ADDR LEN [--ms N] [--count POLLS]
  fmscan diffset PID [--noise-ms N] [--action-wait SECONDS] [--near HEX[:RADIUS]]
                    [--exclude FILE] [--max N]
All commands support --json (watch and transfer-watch emit JSON lines). Errors go to stderr.
Addresses/radii and diff values are hex. Other integers are decimal or 0x-prefixed.
Scans use anonymous/heap rw-private mappings only. --near radius defaults to 0x100.
watch: Ctrl-C stops; --count 0 (default) watches indefinitely.
diffset: samples noise, snapshots baseline, waits for action, then ranks changes.
Without --action-wait it waits for Enter. Ranking/ownership/stride/date are heuristics.
Requires process_vm_readv permission (same user may still be blocked by ptrace policy).
)";
int main(int argc, char** argv) {
    bool json = false; for (int i = 1; i < argc; ++i) if (std::string(argv[i]) == "--json") json = true;
    try {
        if (argc == 2 && std::string(argv[1]) == "--help") { std::cout << HELP; return 0; }
        Args a(argc, argv);
        if (a.pos.size() < 2) throw std::runtime_error("command and PID required; use --help");
        U pid = number(a.pos[1]); if (!pid || pid > INT32_MAX) throw std::runtime_error("invalid PID");
        Memory m(static_cast<pid_t>(pid)); std::signal(SIGINT, stop_handler); std::signal(SIGTERM, stop_handler);
        const auto& cmd = a.pos[0];
        if (cmd == "snap") return cmd_snap(m, a);
        if (cmd == "diff") return cmd_diff(m, a);
        if (cmd == "refs" || cmd == "find") return cmd_search(m, a, cmd == "refs");
        if (cmd == "dump") return cmd_dump(m, a);
        if (cmd == "struct") return cmd_struct(m, a);
        if (cmd == "rtti") return cmd_rtti(m, a);
        if (cmd == "vec") return cmd_vec(m, a);
        if (cmd == "graph") return cmd_graph(m, a);
        if (cmd == "watch") return cmd_watch(m, a);
        if (cmd == "diffset") return cmd_diffset(m, a);
        if (cmd == "entity") return cmd_entity(m, a);
        if (cmd == "player-data") return cmd_player_data(m, a);
        if (cmd == "contract" || cmd == "transfer" || cmd == "transfer-watch") return cmd_contract(m, a);
        throw std::runtime_error("unknown command; use --help");
    } catch (const std::exception& e) {
        if (json) { Json j; j.str("error", e.what()); std::cerr << j.done() << std::endl; }
        else std::cerr << "fmscan: " << e.what() << std::endl;
        return 1;
    }
}
