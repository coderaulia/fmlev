# Quick Tools and Preset Keys plan (from FM26 Studio screenshots, 2026-11-08)

Source: FM26 Studio's player "Quick Tools" sidebar and club "Preset Keys" sidebar (user screenshots), plus a static
read of its decompiled code. Their offsets are leads only. Each tool below needs the evidence ladder
(AGENTS.md Rule 3) before it counts as verified.

**Write policy (user decision, 2026-11-08):** every tool gets a write path, even unverified ones, because testing is
done on a throwaway save. Unverified tools are labelled "experimental" and need the experimental opt-in. Each write
still goes through `apply_transaction` (pre-read, write, readback, rollback). Rule 7 still applies: no vector
resizing or compaction, no allocated objects or strings, no code patches. A tool that can only be built that way is
listed as **blocked**, with the safe alternative.

Status: **done** = shipped and checked in game; **partial** = shipped but incomplete or not checked in game;
**todo** = not built; **blocked** = their method breaks Rule 7.

## Player Quick Tools

| Tool | Status | Our path / plan | Lead from FM26 Studio |
|---|---|---|---|
| Inspire (Max Morale) | done | `morale` preset, record+0x124 (= ACTUAL_PLAYER+0x26c) = 20, screen-verified | P+0x26c = 20 (same field) |
| Heal Player | partial | `heal` sets condition and sharpness to 10000. Never touch the injury vector (P+0x248). | condition P+0x258, jadedness P+0x25a = 0, sharpness P+0x25c, morale, P+0x21a = 0 (unknown; map first) |
| Max Match Fitness | todo | Add jadedness P+0x25a = 0 to the heal path; show Condition, Sharpness, Jadedness and Readiness in the header like theirs | P+0x258/+0x25a/+0x25c |
| Destroy | todo | Morale 1 and condition drained (P+0x258 low), verified fields only | UI "demoralize and drain condition" |
| Freeze Player | todo | Server-side job that re-writes chosen verified fields every ~1.5 s while enabled. Stops on detach or identity mismatch. | `PlayerFreezerService`, 1.5 s timer |
| Remove Unhappiness | todo | Map first: their writes go to (P+0x330 -> obj)+0x5a / +0x5c = 100, +0x57 byte, and a list entry +0xc = 0. Happiness is still unmapped (see AGENTS "Promises: parked"). | `RemovePlayerUnhappiness` |
| Remove Ban & Suspensions | todo | Scalar-only zeroing of ban record fields (+0x16 u16, +0x18/+0x1a/+0x1c/+0x1d). Their removal of the record from the list is **blocked**. | `ClearPlayerBansAndSuspensions` |
| Trained at Club (HGC) / Trained at Nation (HGN) | in progress (agent) | **User override of Rule 7 (2026-11-08: "USE THEIR METHOD IS OKAY")**: append a 16-byte relationship record `{club/nation ptr, i16 0, cat, sub, type 1, ...}` to the person relationship vector at `(ACTUAL_PLAYER+0x288)+0x78` (begin/end/cap). Use spare capacity when there is any; otherwise allocate fresh memory in the game process (ptrace-injected `mmap`, the Linux equivalent of their `VirtualAllocEx`), copy the old records, and swap begin/end/cap. Live-verified layout: Foden has Man City cat 1/sub 72 and England cat 8/sub 70; Asencio has R. Madrid and Spain. Risk: the game may later free or grow the foreign array. | `EnsureHomeGrownStatus` + `AppendRelationRecord` |
| Register for UEFA | todo | Map per-player continental registration. We only have the manager bypass bit 0x100 today. | `RegisterPlayerForUefa` |
| Register (Domestic) | todo | Per-player domestic registration (lead: player +0x130 chain). Register Anytime opens the window only. | `RegisterPlayerForDomestic(Division)` |
| Move to another club | partial | `/api/transfer/move`, checked in game; no fee/history records (see Career history notes). Long term: use deal objects (`native_deals`). | they edit deal objects |
| Recall from Loan | partial | `/api/player/loan/terminate` exists, never run in game | `RecallPlayerFromLoan` |
| Reload Values | done | Load from game | - |
| Hidden attributes (Consistency, Dirtiness, Important Matches, Injury Proneness, Versatility) | check | Confirm they are in our 60-attribute editor and screen-verified | attribute tab |

## Club Preset Keys

| Key | Status | Our path / plan | Lead |
|---|---|---|---|
| Heal Squad | partial | `heal_squad`; add jadedness | as Heal |
| Inspire Squad | partial | `morale_squad` (write path verified on one player, squad run not checked live) | P+0x26c |
| Remove Unhappiness (squad) | todo | After the player version is mapped | `RemoveSquadUnhappiness` |
| Destroy Rivals | todo | Squad version of Destroy, applied to any club | `BtnDestroySquad` |
| Max Fitness | todo | Squad version of Max Match Fitness | `MaxMatchFitness` |
| Tactical Familiarity | done | `POST /api/player/familiarity {squad}`, verified in game | - |
| Partnerships (100%) | todo | Map the per-player partnership list (lead P+0x328) and edit values in place; never resize | `MaximizeTacticalPartnerships` |
| Team Cohesion | todo | Lead P+0x17b = 100 plus jadedness 0. Map P+0x17b first. | `MaximizeSquadCohesion` |
| Clear Bans | todo | Squad version of Remove Ban (scalar-only) | `ClearSquadBansAndSuspensions` |
| Register Domestic / Register UEFA (squad) | todo | Squad versions of the player tools | `RegisterSquadForDomestic` |
| Unlock Registration | done | Register Anytime (manager dword +0x404 bits 0x100/0x200 plus stage gate), verified | same dword (+0x405) |
| UEFA Hook | blocked | Theirs is a code cave (RWX page plus a hook at RVA 0x1f9db60). We never patch game code. Alternative: per-player UEFA registration above. | `SquadEligibilityHookService` |
| Licensed Kits | todo | Scalar byte (club or team +0x18 -> obj +0x2e) | `WriteLicensedKit` |
| Unsackable | todo | Human manager dword +0x404 **bit 0x01** (the same dword as Register Anytime; set the bit only, keep 0x100/0x200) | `ManagerSecurityService`, +0x404 byte = 1 |
| Renew (+3 yrs) | partial | `/api/player/contract/extend {years}` per player; add a squad version | - |
| Release / Expire | partial | Terminate contract (checked in game, per player); squad version is high risk | - |
| Apply to all squads | todo | Option to include the club's other teams (B, U19, ...) via the club's team vector | checkbox |

## In progress on agent branches (2026-11-08)
Transfer deals, club embargo and financial statements, bank loans and sponsors, board objectives and club status,
competition extras (pitch sizes, trophy type, original reputation, native prize table, league table, awards).

## Suggested order
1. Cheap scalars on known objects: Unsackable, jadedness / Max Fitness, Destroy and Destroy Rivals, Licensed Kits,
   squad Renew.
2. Freeze Player (a job over existing verified writes).
3. Mapping work: bans, team cohesion, partnerships, unhappiness, per-player registration.
4. Blocked items: only if a Rule 7-safe path turns up.
