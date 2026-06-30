# TEZZERA — PRINCIPLES
> These are non-negotiable. They never change.
> If a decision conflicts with a principle, the decision is wrong.

---

## P1 — STRICT UNDERNEATH, INVISIBLE ON TOP
The framework enforces rules at compile time.
Users write clean code and it works.
Errors are caught before they ever run.
Users never feel the strictness — they only feel the safety.

**Violation example:**
- Adding a runtime check where a compile-time check is possible → WRONG
- Showing a cryptic compiler error → WRONG (must be friendly, actionable)

---

## P2 — EFFICIENCY IS NOT OPTIONAL
120fps is the target. Always.
No feature ships if it compromises frame budget.
Performance is measured, not assumed.

**Violation example:**
- Merging layout code without benchmarking → WRONG
- Adding allocation in the hot render path → WRONG

---

## P3 — TRACING IS ARCHITECTURE
Every system emits TezzeraTrace events.
No system merges without its trace emissions.
Zero cost in production — #[cfg(debug_assertions)] everywhere.

**Violation example:**
- Implementing atom without AtomRead/AtomWrite traces → WRONG
- Adding trace overhead visible in release builds → WRONG

---

## P4 — ONE WAY TO DO THINGS
For every problem, TEZZERA has one right way.
No debates, no multiple patterns for the same thing.
The right way is documented in DECISIONS.md.

**Violation example:**
- Adding a second state API because it feels nicer → WRONG
- Providing two layout systems → WRONG

---

## P5 — COMPILE TIME OVER RUNTIME
If something can be caught at compile time, it must be.
Runtime errors are a last resort, never a design choice.

**Violation example:**
- Panicking at runtime for missing required prop → WRONG
- Using Any or dynamic dispatch where generics work → WRONG

---

## P6 — CRATE BOUNDARIES ARE SACRED
Each crate has a contract defined in CRATE_CONTRACTS.md.
A crate never reaches into another crate's internals.
Dependencies only go downward in the stack.

**Violation example:**
- tezzera-nav depending on tezzera-render internals → WRONG
- tezzera-core importing from tezzera-widgets → WRONG

---

## P7 — NO SYSTEM IS AN ISLAND
Every system is designed knowing it connects to every other.
Layout knows about scroll. State knows about tracing.
Navigation knows about animation.
Design in isolation = problems at integration.

**Violation example:**
- Designing atom batching without knowing about the refresh engine → WRONG

---

## P8 — DOCUMENT BEFORE CODE
Every new feature needs its decision documented first.
Code without a decision behind it is a liability.
If it is not in DECISIONS.md, do not build it.

**Violation example:**
- Writing a new API without adding it to DECISIONS.md → WRONG
- Changing an API without updating the decision → WRONG

---

## P9 — MISTAKES ARE EXPENSIVE, VERIFICATION IS CHEAP
Every step has a checklist. Every checklist gets run.
Skipping a checklist to save time costs more time.
Slow is smooth. Smooth is fast.

**Violation example:**
- Committing without running CHECKLIST_BEFORE_COMMIT.md → WRONG
- Merging without running CHECKLIST_BEFORE_MERGE.md → WRONG

---

## P10 — THE MOSAIC METAPHOR GUIDES EVERYTHING
Each component is a tile. Simple, precise, self-contained.
Together they form something beautiful.
If a design makes components complicated or interdependent — rethink it.

**Violation example:**
- Component that requires knowledge of its parent → WRONG
- Component with hidden global side effects → WRONG
