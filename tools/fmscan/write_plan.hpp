#pragma once
#include "entity_details.hpp"
#include <fcntl.h>
#include <sys/file.h>
#include <sys/stat.h>

struct WritePolicy {
  std::string kind, field;
  bool experimental;
};
inline const std::vector<WritePolicy> WRITE_POLICY{
    {"club", "balance", false},
    {"club", "transfer_budget", false},
    {"club", "training_facilities", true},
    {"club", "youth_facilities", true},
    {"club", "junior_coaching", true},
    {"club", "youth_recruitment", true},
    {"club", "corporate_facilities", true},
    {"club", "reputation", true},
    {"club", "world_reputation", true},
    {"nation", "men.youth_rating", true},
    {"nation", "women.youth_rating", true},
    {"nation", "men.reputation", true},
    {"nation", "women.reputation", true},
    {"nation", "economic_factor", true},
    {"competition", "reputation", true}};
inline const WritePolicy &policy(const std::string &kind,
                                 const std::string &key) {
  for (auto &p : WRITE_POLICY)
    if (p.kind == kind && p.field == key)
      return p;
  throw std::runtime_error("field is not writable: " + kind + "." + key +
                           " (strings, structural pointers, derived/uncertain "
                           "fields and unmapped fields are rejected)");
}
inline int64_t signed_number(const std::string &s) {
  int64_t n = 0;
  auto [p, ec] = std::from_chars(s.data(), s.data() + s.size(), n);
  if (ec != std::errc{} || p != s.data() + s.size() || s.empty())
    throw std::runtime_error("invalid signed decimal value: " + s);
  return n;
}
inline U process_start(pid_t pid) {
  std::ifstream f("/proc/" + std::to_string(pid) + "/stat");
  std::string line;
  std::getline(f, line);
  auto end = line.rfind(')');
  if (!f || end == std::string::npos)
    throw std::runtime_error("cannot read process start identity");
  std::istringstream s(line.substr(end + 2));
  std::string v;
  for (int i = 0; i <= 19; ++i)
    if (!(s >> v))
      throw std::runtime_error("invalid process stat");
  return number(v);
}
struct Edit {
  std::string key;
  U address;
  size_t width;
  bool is_signed;
  int64_t before, after;
  bool experimental;
};
struct WritePlan {
  pid_t pid = 0;
  U start = 0, root = 0;
  std::string kind, name, state = "prepared";
  uint32_t index = 0, uid = 0;
  std::vector<Edit> edits;
  Json json() const {
    Json j;
    j.str("kind", kind);
    j.str("name", name);
    j.num("index", index);
    j.num("uid", uid);
    j.num("pid", pid);
    j.num("process_start_ticks", start);
    j.addr("root", root);
    j.str("state", state);
    std::vector<std::string> rows;
    for (auto &e : edits) {
      Json r;
      r.str("field", e.key);
      r.addr("address", e.address);
      r.num("width", e.width);
      r.raw("before", std::to_string(e.before));
      r.raw("after", std::to_string(e.after));
      r.raw("experimental", e.experimental ? "true" : "false");
      rows.push_back(r.done());
    }
    j.raw("changes", array(rows));
    j.str("game_visibility", "unverified until checked in game");
    return j;
  }
};
inline std::map<std::string, int64_t> parse_changes(const std::string &s) {
  std::map<std::string, int64_t> out;
  std::istringstream f(s);
  std::string part;
  while (std::getline(f, part, ',')) {
    auto eq = part.find('=');
    if (eq == std::string::npos || eq == 0)
      throw std::runtime_error("use --set field=value,field=value");
    auto key = part.substr(0, eq);
    if (!out.emplace(key, signed_number(part.substr(eq + 1))).second)
      throw std::runtime_error("duplicate requested field");
  }
  if (out.empty() || out.size() > 32 || s.ends_with(','))
    throw std::runtime_error("invalid change count/list");
  return out;
}
inline NativeEntity unique_entity(const Memory &m, const NativeCatalog &c,
                                  const std::string &kind, uint32_t index,
                                  uint32_t uid) {
  std::set<U> vt;
  for (auto &[address, t] : c.tables)
    if (t.kind == kind && !t.subobject_offset)
      vt.insert(address);
  std::vector<NativeEntity> found;
  char *av[] = {const_cast<char *>("fmwrite")};
  Args args(1, av);
  auto stats = scan(m, Near(args), [&](U base, const uint8_t *b, size_t n) {
    for (size_t i = 0; i + 8 <= n; i += 8) {
      if (!vt.contains(word(b + i)))
        continue;
      std::array<uint32_t, 3> ids{};
      if (!m.read(base + i + 8, ids) || ids[0] != index || ids[1] != uid ||
          ids[2] != uid)
        continue;
      auto e = read_entity(m, c, base + i, kind);
      if (e)
        found.push_back(*e);
    }
  });
  require_scan(stats);
  if (stats.unreadable)
    std::cerr << stats.unreadable
              << " unreadable chunks excluded from entity search\n";
  if (found.size() != 1)
    throw std::runtime_error(
        "write requires exactly one native identity match, found " +
        std::to_string(found.size()));
  return found[0];
}
inline WritePlan make_plan(const Memory &m, const NativeCatalog &catalog,
                           const std::string &kind, uint32_t index,
                           uint32_t uid,
                           const std::map<std::string, int64_t> &changes) {
  if (!entity_layout_build(m, catalog))
    throw std::runtime_error("unknown native DLL build; writes disabled");
  for (auto &[key, v] : changes) {
    (void)v;
    policy(kind, key);
  }
  auto e = unique_entity(m, catalog, kind, index, uid);
  auto d = entity_details(m, catalog, e);
  WritePlan p;
  p.pid = m.pid;
  p.start = process_start(m.pid);
  p.root = e.address;
  p.kind = kind;
  p.name = e.name;
  p.index = index;
  p.uid = uid;
  for (auto &[key, value] : changes) {
    auto it = d.scalars.find(key);
    if (it == d.scalars.end())
      throw std::runtime_error("native field is unavailable: " + key);
    auto f = it->second;
    int64_t low = key == "transfer_budget" ? 0 : f.low;
    if (f.value < f.low || f.value > f.high || value < low || value > f.high)
      throw std::runtime_error("value outside native write range: " + key);
    if (f.width != 1 && f.width != 2 && f.width != 4)
      throw std::runtime_error("unsupported native scalar width");
    auto r = m.region(f.address, f.width);
    if (!r || !r->heap())
      throw std::runtime_error("field is not in writable private heap");
    p.edits.push_back({key, f.address, f.width, f.is_signed, f.value, value,
                       policy(kind, key).experimental});
  }
  return p;
}
inline void fd_write(int fd, const std::string &text) {
  size_t done = 0;
  while (done < text.size()) {
    ssize_t n = write(fd, text.data() + done, text.size() - done);
    if (n < 0 && errno == EINTR)
      continue;
    if (n <= 0)
      throw std::runtime_error("journal write failed");
    done += n;
  }
  if (fsync(fd))
    throw std::runtime_error("journal fsync failed");
}
inline std::string plan_text(const WritePlan &p) {
  std::ostringstream s;
  s << "FMLE_WRITE_PLAN_1\nidentity " << p.pid << " " << p.start << " "
    << p.kind << " " << p.index << " " << p.uid << " " << hex(p.root) << " "
    << std::quoted(p.name) << "\n";
  for (auto &e : p.edits)
    s << "edit " << e.key << " " << hex(e.address) << " " << e.width << " "
      << e.is_signed << " " << e.before << " " << e.after << " "
      << e.experimental << "\n";
  return s.str() + "state prepared\n";
}
inline void save_plan(const std::string &path, const WritePlan &p) {
  int fd = open(path.c_str(),
                O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0600);
  if (fd < 0)
    throw std::runtime_error(
        "plan must be a new writable file (not overwritten): " + path);
  try {
    fd_write(fd, plan_text(p));
    close(fd);
  } catch (...) {
    close(fd);
    throw;
  }
  // Persist the directory entry too, before a later command can apply this
  // plan.
  auto parent = std::filesystem::absolute(path).parent_path();
  int dir = open(parent.c_str(), O_RDONLY | O_DIRECTORY | O_CLOEXEC);
  if (dir < 0)
    throw std::runtime_error("cannot open plan directory for durability");
  int rc = fsync(dir);
  close(dir);
  if (rc)
    throw std::runtime_error("plan directory fsync failed");
}
inline WritePlan load_plan(const std::string &path) {
  std::ifstream f(path);
  std::string line;
  WritePlan p;
  if (!std::getline(f, line) || line != "FMLE_WRITE_PLAN_1")
    throw std::runtime_error("invalid write plan header");
  if (!std::getline(f, line))
    throw std::runtime_error("missing plan identity");
  std::istringstream identity(line);
  std::string tag, root, extra;
  if (!(identity >> tag >> p.pid >> p.start >> p.kind >> p.index >> p.uid >>
        root >> std::quoted(p.name)) ||
      tag != "identity" || (identity >> extra) || p.pid <= 0 || !p.index ||
      p.index > INT32_MAX || !p.uid)
    throw std::runtime_error("invalid plan identity");
  p.root = number(root, 16);
  std::set<std::string> keys;
  bool state = false;
  while (std::getline(f, line)) {
    if (line.size() > 4096)
      throw std::runtime_error("oversize journal line");
    std::istringstream s(line);
    s >> tag;
    if (tag == "edit") {
      Edit e{};
      std::string address;
      if (state ||
          !(s >> e.key >> address >> e.width >> e.is_signed >> e.before >>
            e.after >> e.experimental) ||
          (s >> extra) || !keys.insert(e.key).second || p.edits.size() >= 32)
        throw std::runtime_error("invalid plan edit");
      e.address = number(address, 16);
      const auto &rule = policy(p.kind, e.key);
      if (e.experimental != rule.experimental ||
          (e.width != 1 && e.width != 2 && e.width != 4))
        throw std::runtime_error("plan policy/width mismatch");
      p.edits.push_back(e);
    } else if (tag == "state") {
      if (!(s >> p.state) || (s >> extra))
        throw std::runtime_error("invalid plan state");
      state = true;
    } else if (tag == "progress") {
      size_t index = 0;
      if (!state || !(s >> index) || (s >> extra) || index > p.edits.size())
        throw std::runtime_error("invalid progress record");
    } else
      throw std::runtime_error("unexpected journal record");
  }
  const std::set<std::string> states{
      "prepared", "applying",    "applied",        "restoring",
      "restored", "rolled_back", "rollback_failed"};
  if (!f.eof() || !state || p.edits.empty() || !states.contains(p.state))
    throw std::runtime_error("incomplete or invalid write plan");
  return p;
}
