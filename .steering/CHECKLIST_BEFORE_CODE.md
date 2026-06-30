# TEZZERA — CHECKLIST BEFORE CODE
> Run this before writing ANY new code.
> Every item must be checked. No exceptions.

---

## THE CHECKLIST

### 1. Am I in the right phase?
```
□ I know which phase we are in (currently: Phase 1)
□ What I am about to build is in this phase's plan
□ I am not building anything in the DO NOT LIST
```

### 2. Is this already decided?
```
□ I checked DECISIONS.md for relevant decisions
□ If decided → I am following the decision exactly
□ If not decided → I will add it to DECISIONS.md FIRST
□ I am not re-debating a locked decision
```

### 3. Do I know the crate boundary?
```
□ I know which crate this code belongs in
□ I checked CRATE_CONTRACTS.md for that crate
□ My code stays within the crate's contract
□ I am not adding a dependency that goes UP the hierarchy
```

### 4. Do I know the names?
```
□ I checked NAMING.md for correct names
□ Types are PascalCase
□ Functions are snake_case
□ Constants are SCREAMING_SNAKE_CASE
□ I am not inventing new naming patterns
```

### 5. Will this be traceable?
```
□ If this is a new system → it emits TezzeraTrace events
□ I know which TezzeraTrace variants to emit
□ Trace emissions are behind #[cfg(debug_assertions)]
□ No trace overhead in release build
```

### 6. Does this need a test?
```
□ I have written the test BEFORE the implementation
   (or at minimum alongside it)
□ The test covers the happy path
□ The test covers at least one error case
□ The test covers edge cases I can think of
```

### 7. Does this touch the principles?
```
□ P1 — Is this strict underneath, invisible on top?
□ P2 — Does this maintain 60fps (Phase 1) / 120fps (Phase 2+)?
□ P3 — Does this emit traces?
□ P4 — Is there ONE way to do this?
□ P5 — Am I using compile-time checks where possible?
□ P6 — Am I staying within crate boundaries?
□ P7 — Have I considered how this connects to other systems?
□ P8 — Is there a decision in DECISIONS.md backing this?
□ P9 — Am I running all checklists?
□ P10 — Is this component simple and self-contained?
```

### 8. Any unsafe code?
```
□ If using unsafe → I have a SAFETY comment explaining why
□ The unsafe block is as small as possible
□ I have considered all invariants
```

---

## IF ANY BOX IS UNCHECKED

Stop. Do not write code.
Fix the unchecked item first.
Then re-run this checklist from the top.

---

## QUICK VERSION (for tiny changes)

For changes under 10 lines (typos, doc fixes, tiny tweaks):
```
□ Is this in scope for current phase?
□ Does it follow NAMING.md?
□ Does it have a test if needed?
```
