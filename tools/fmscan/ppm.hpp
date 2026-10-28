#pragma once
// Player preferred moves (PPM): read-only 64-bit bitset at player record + 0x200
// (= ACTUAL_PLAYER + 0x348). Only bits with screen evidence are named; every
// other set bit is reported as an unnamed bit, never guessed.
#include "entities.hpp"

inline constexpr U PPM_BITSET_OFFSET = 0x200;
struct PpmBit {
  unsigned bit;
  const char *name;
  const char *evidence;
};
// A bit is listed only when the on-screen profiles prove it. Bit 39 is the only
// bit set for Mbappe (Cuts Inside From Both Wings) and clear for Yamal and
// Haaland, whose screens do not list that move. Bits 35/51 (Tries Tricks /
// Runs With Ball Often) and 11/14/58 (Curls Ball / Uses Outside Of Foot / Cuts
// Inside From Right Wing) are narrowed to sets but not ordered: they stay unnamed.
inline const std::array<PpmBit, 1> PPM_VERIFIED_BITS{
    PpmBit{39, "Cuts Inside From Both Wings",
           "set only for Mbappe; clear for Yamal and Haaland (screen profiles)"}};
inline const char *ppm_bit_name(unsigned bit) {
  for (const auto &v : PPM_VERIFIED_BITS)
    if (v.bit == bit)
      return v.name;
  return nullptr;
}
struct PpmRead {
  bool readable = false;
  U bitset = 0;
};
inline PpmRead read_ppm(const Memory &m, U record) {
  PpmRead r;
  // The bitset sits in the same private heap object as the validated record.
  auto region = m.region(record + PPM_BITSET_OFFSET, 8);
  r.readable = region && region->heap() &&
               m.read(record + PPM_BITSET_OFFSET, r.bitset);
  if (!r.readable)
    r.bitset = 0;
  return r;
}
inline Json ppm_json(const PpmRead &r) {
  Json j;
  j.str("status", "partial_bit_table");
  j.addr("offset_from_record", PPM_BITSET_OFFSET);
  if (!r.readable) {
    j.raw("bitset", "null");
    j.str("unavailable_reason", "bitset bytes unreadable");
    return j;
  }
  j.addr("bitset", r.bitset);
  j.num("popcount", __builtin_popcountll(r.bitset));
  std::vector<std::string> bits, named;
  unsigned unnamed = 0;
  for (unsigned b = 0; b < 64; ++b) {
    if (!(r.bitset >> b & 1))
      continue;
    const char *name = ppm_bit_name(b);
    Json e;
    e.num("bit", b);
    e.raw("name", name ? quote(name) : "null");
    e.raw("verified", name ? "true" : "false");
    bits.push_back(e.done());
    if (name)
      named.push_back(quote(name));
    else
      ++unnamed;
  }
  j.raw("set_bits", array(bits));
  j.raw("named_moves", array(named));
  j.num("unnamed_set_bits", unnamed);
  j.str("verification",
        "bit-to-move table incomplete: only bits listed with verified=true "
        "have screen evidence; no write support");
  return j;
}
inline std::string ppm_text(const PpmRead &r) {
  if (!r.readable)
    return "Unavailable";
  std::ostringstream s;
  s << hex(r.bitset) << " (" << __builtin_popcountll(r.bitset) << " set)";
  for (unsigned b = 0; b < 64; ++b)
    if (r.bitset >> b & 1) {
      const char *name = ppm_bit_name(b);
      s << "\n  bit " << b << ": " << (name ? name : "unknown (unverified)");
    }
  return s.str();
}
