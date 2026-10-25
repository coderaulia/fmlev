// Explicit transactional writer. fmscan remains read-only.
#include "write_transaction.hpp"
int main(int argc, char **argv) {
  bool json = false;
  for (int i = 1; i < argc; ++i)
    if (std::string(argv[i]) == "--json")
      json = true;
  try {
    // Keep the opt-in local to this executable instead of changing scanner
    // options.
    std::vector<char *> filtered{argv[0]};
    bool experimental = false;
    for (int i = 1; i < argc; ++i) {
      if (std::string(argv[i]) == "--experimental") {
        if (experimental)
          throw std::runtime_error("duplicate --experimental");
        experimental = true;
      } else
        filtered.push_back(argv[i]);
    }
    Args a(filtered.size(), filtered.data());
    if (a.pos.empty() || a.pos[0] == "help") {
      std::cout
          << "fmwrite plan PID club|nation|competition INDEX UID --set "
             "field=value,... --plan NEW_FILE [--json]\n"
          << "fmwrite apply PLAN_FILE [--experimental] [--json]\nfmwrite "
             "restore PLAN_FILE [--json]\nfmwrite show PLAN_FILE [--json]\n"
          << "plan is read-only; apply/restore write only guarded allowlisted "
             "scalar fields. Use a test save.\n";
      return 0;
    }
    Json result;
    auto cmd = a.pos[0];
    if (cmd == "plan") {
      a.allow({"--set", "--plan"}, 5);
      if (experimental || !a.has("--set") || !a.has("--plan"))
        throw std::runtime_error(
            "plan requires --set and --plan; --experimental belongs to apply");
      U pid = number(a.pos[1]), index = number(a.pos[3]),
        uid = number(a.pos[4]);
      if (!pid || pid > INT32_MAX || !index || index > INT32_MAX || !uid ||
          uid > UINT32_MAX)
        throw std::runtime_error("invalid process/entity identity");
      auto changes = parse_changes(a.get("--set"));
      for (auto &[key, value] : changes) {
        (void)value;
        policy(a.pos[2], key);
      }
      Memory m(pid);
      NativeCatalog catalog(m);
      auto p = make_plan(m, catalog, a.pos[2], index, uid, changes);
      save_plan(a.get("--plan"), p);
      result = p.json();
      result.str("plan_file",
                 std::filesystem::absolute(a.get("--plan")).string());
    } else if (cmd == "apply" || cmd == "restore" || cmd == "show") {
      a.allow({}, 2);
      if (experimental && cmd != "apply")
        throw std::runtime_error("--experimental belongs to apply");
      result = cmd == "show"
                   ? load_plan(a.pos[1]).json()
                   : transact(a.pos[1], cmd == "restore", experimental);
    } else
      throw std::runtime_error("unknown command; use fmwrite help");
    emit(a, result, result.done());
    return 0;
  } catch (const std::exception &e) {
    if (json) {
      Json j;
      j.str("error", e.what());
      std::cerr << j.done() << "\n";
    } else
      std::cerr << "fmwrite: " << e.what() << "\n";
    return 1;
  }
}
