#pragma once
#include "write_plan.hpp"

struct ProcessWriteLock {
  int fd = -1;
  explicit ProcessWriteLock(const WritePlan &p) {
    auto path = std::filesystem::temp_directory_path() /
                ("fmwrite-" + std::to_string(p.pid) + "-" +
                 std::to_string(p.start) + ".lock");
    fd = open(path.c_str(), O_RDWR | O_CREAT | O_NOFOLLOW | O_CLOEXEC, 0600);
    struct stat st{};
    if (fd < 0 || fstat(fd, &st) || !S_ISREG(st.st_mode) ||
        st.st_uid != geteuid() || flock(fd, LOCK_EX | LOCK_NB)) {
      if (fd >= 0)
        close(fd);
      fd = -1;
      throw std::runtime_error(
          "cannot acquire exclusive per-process write lock");
    }
  }
  ~ProcessWriteLock() {
    if (fd >= 0)
      close(fd);
  }
  ProcessWriteLock(const ProcessWriteLock &) = delete;
};
inline std::vector<uint8_t> scalar_bytes(int64_t v, size_t width) {
  std::vector<uint8_t> b(width);
  U raw = static_cast<U>(v);
  for (size_t i = 0; i < width; ++i)
    b[i] = uint8_t(raw >> (i * 8));
  return b;
}
inline bool same_bytes(const Memory &m, U address, int64_t value,
                       size_t width) {
  auto expected = scalar_bytes(value, width);
  std::vector<uint8_t> actual(width);
  return m.read(address, actual.data(), width) && actual == expected;
}
inline std::optional<int64_t> partial_value(const Memory &m, const Edit &e) {
  auto before = scalar_bytes(e.before, e.width),
       after = scalar_bytes(e.after, e.width);
  std::vector<uint8_t> current(e.width);
  if (!m.read(e.address, current.data(), current.size()))
    return {};
  bool matches = false;
  for (size_t n = 0; n <= e.width; ++n)
    for (bool reverse : {false, true}) {
      auto mixed = reverse ? after : before;
      auto &prefix = reverse ? before : after;
      std::copy_n(prefix.begin(), n, mixed.begin());
      if (mixed == current)
        matches = true;
    }
  if (!matches)
    return {};
  U raw = 0;
  for (size_t i = 0; i < current.size(); ++i)
    raw |= U(current[i]) << (i * 8);
  if (e.is_signed && (current.back() & 0x80))
    raw |= UINT64_MAX << (e.width * 8);
  return std::bit_cast<int64_t>(raw);
}
using WriteFunction =
    std::function<ssize_t(pid_t, U, const std::vector<uint8_t> &)>;
inline ssize_t native_write(pid_t pid, U addr,
                            const std::vector<uint8_t> &bytes) {
  iovec local{const_cast<uint8_t *>(bytes.data()), bytes.size()},
      remote{reinterpret_cast<void *>(addr), bytes.size()};
  return process_vm_writev(pid, &local, 1, &remote, 1, 0);
}
inline Details validate_owner(const Memory &m, const NativeCatalog &catalog,
                              const WritePlan &p) {
  if (process_start(p.pid) != p.start || !entity_layout_build(m, catalog))
    throw std::runtime_error("process instance or native DLL build changed");
  auto e = read_entity(m, catalog, p.root, p.kind);
  if (!e || e->index != p.index || e->uid != p.uid || e->name != p.name)
    throw std::runtime_error("native entity identity changed");
  auto d = entity_details(m, catalog, *e);
  for (auto &edit : p.edits) {
    policy(p.kind, edit.key);
    auto it = d.scalars.find(edit.key);
    if (it == d.scalars.end())
      throw std::runtime_error(
          "native write owner/field no longer available: " + edit.key);
    auto f = it->second;
    int64_t low = edit.key == "transfer_budget" ? 0 : f.low;
    if (f.address != edit.address || f.width != edit.width ||
        f.is_signed != edit.is_signed || edit.before < f.low ||
        edit.before > f.high || edit.after < low || edit.after > f.high)
      throw std::runtime_error(
          "plan target/type/range no longer matches native field: " + edit.key);
  }
  return d;
}
inline void ensure_exclusive_subrecord(const Memory &m,
                                       const NativeCatalog &catalog,
                                       const WritePlan &p) {
  if (p.kind != "club")
    return;
  bool details = false, finance = false;
  for (auto &e : p.edits) {
    if (e.key == "training_facilities" || e.key == "youth_facilities" ||
        e.key == "junior_coaching" || e.key == "youth_recruitment")
      details = true;
    if (e.key == "balance" || e.key == "transfer_budget" ||
        e.key == "corporate_facilities")
      finance = true;
  }
  if (!details && !finance)
    return;
  U a = pointer_field(m, p.root, 0x100), b = pointer_field(m, p.root, 0x150);
  std::set<U> tables;
  for (auto &[vt, t] : catalog.tables)
    if (t.kind == "club" && !t.subobject_offset)
      tables.insert(vt);
  char *av[] = {const_cast<char *>("fmwrite")};
  Args args(1, av);
  size_t owners = 0;
  auto stats = scan(m, Near(args), [&](U base, const uint8_t *bytes, size_t n) {
    for (size_t i = 0; i + 8 <= n; i += 8) {
      if (!tables.contains(word(bytes + i)))
        continue;
      U root = base + i;
      if ((details && pointer_field(m, root, 0x100) == a) ||
          (finance && pointer_field(m, root, 0x150) == b)) {
        auto e = read_entity(m, catalog, root, "club");
        if (e) {
          if (root != p.root)
            throw std::runtime_error("club subrecord is shared by another "
                                     "native club; write refused");
          ++owners;
        }
      }
    }
  });
  require_scan(stats);
  if (owners != 1)
    throw std::runtime_error("cannot prove unique native club subrecord owner");
}
inline Json transact(const std::string &path, bool restore,
                     bool allow_experimental,
                     WriteFunction writer = native_write) {
  auto plan = load_plan(path);
  ProcessWriteLock lock(plan);
  if (!restore && !allow_experimental)
    for (auto &e : plan.edits)
      if (e.experimental)
        throw std::runtime_error(
            "new scalar writes require --experimental; game "
            "visibility/durability is not yet verified");
  if (!restore && plan.state != "prepared" && plan.state != "restored" &&
      plan.state != "rolled_back")
    throw std::runtime_error(
        "plan is not ready to apply; restore an interrupted transaction first");
  Memory m(plan.pid);
  NativeCatalog catalog(m);
  if (process_start(plan.pid) != plan.start)
    throw std::runtime_error("PID belongs to a different process instance");
  auto current = unique_entity(m, catalog, plan.kind, plan.index, plan.uid);
  if (current.address != plan.root)
    throw std::runtime_error("native root moved; old plan rejected");
  validate_owner(m, catalog, plan);
  ensure_exclusive_subrecord(m, catalog, plan);
  std::vector<Edit> work;
  for (auto e : plan.edits) {
    int64_t desired = restore ? e.before : e.after;
    if (restore && same_bytes(m, e.address, e.before, e.width))
      continue;
    int64_t expected = restore ? e.after : e.before;
    if (!same_bytes(m, e.address, expected, e.width)) {
      auto partial =
          restore && (plan.state == "applying" || plan.state == "restoring" ||
                      plan.state == "rollback_failed")
              ? partial_value(m, e)
              : std::nullopt;
      if (!partial)
        throw std::runtime_error(
            "current value differs from plan; not overwritten: " + e.key);
      expected = *partial;
    }
    e.before = expected;
    e.after = desired;
    work.push_back(e);
  }
  int journal =
      open(path.c_str(), O_WRONLY | O_APPEND | O_NOFOLLOW | O_CLOEXEC);
  if (journal < 0)
    throw std::runtime_error("cannot open plan journal for append");
  std::vector<Edit> touched;
  std::string failure;
  try {
    fd_write(journal,
             std::string("state ") + (restore ? "restoring\n" : "applying\n"));
    for (auto &e : work) {
      validate_owner(m, catalog, plan);
      if (!same_bytes(m, e.address, e.before, e.width))
        throw std::runtime_error("value changed before write: " + e.key);
      if (e.before == e.after)
        continue;
      touched.push_back(
          e); // Includes partial/failed writes in rollback coverage.
      auto bytes = scalar_bytes(e.after, e.width);
      ssize_t n = writer(plan.pid, e.address, bytes);
      if (n != static_cast<ssize_t>(bytes.size()))
        throw std::runtime_error("short/failed native write: " + e.key);
      if (!same_bytes(m, e.address, e.after, e.width))
        throw std::runtime_error("native write readback mismatch: " + e.key);
      fd_write(journal, "progress " + std::to_string(touched.size()) + "\n");
    }
    for (int poll = 0; poll < 3; ++poll) {
      pause_ms(100);
      validate_owner(m, catalog, plan);
      for (auto &e : plan.edits)
        if (!same_bytes(m, e.address, restore ? e.before : e.after, e.width))
          throw std::runtime_error(
              "native value changed during verification: " + e.key);
    }
    fd_write(journal,
             std::string("state ") + (restore ? "restored\n" : "applied\n"));
    close(journal);
    plan.state = restore ? "restored" : "applied";
    auto result = plan.json();
    result.num("changed_fields", touched.size());
    result.num("readback_polls", 3);
    return result;
  } catch (const std::exception &error) {
    failure = error.what();
  }
  bool rollback_ok = true;
  for (auto it = touched.rbegin(); it != touched.rend(); ++it) {
    try {
      validate_owner(m, catalog, plan);
      if (same_bytes(m, it->address, it->before, it->width))
        continue;
      if (!partial_value(m, *it)) {
        rollback_ok = false;
        continue;
      }
      auto bytes = scalar_bytes(it->before, it->width);
      if (writer(plan.pid, it->address, bytes) !=
              static_cast<ssize_t>(bytes.size()) ||
          !same_bytes(m, it->address, it->before, it->width))
        rollback_ok = false;
    } catch (...) {
      rollback_ok = false;
    }
  }
  try {
    fd_write(journal,
             std::string("state ") +
                 (rollback_ok ? (restore ? plan.state + "\n" : "rolled_back\n")
                              : "rollback_failed\n"));
  } catch (...) {
    rollback_ok = false;
  }
  close(journal);
  throw std::runtime_error(
      failure +
      (rollback_ok
           ? "; rollback verified"
           : "; ROLLBACK NOT VERIFIED; preserve journal and inspect memory"));
}
