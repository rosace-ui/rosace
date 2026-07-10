# ROSACE — CHECKLIST BEFORE COMMIT
> Run this before every git commit.
> Every item must pass. No exceptions.

---

## THE CHECKLIST

### 1. Does it compile?
```
□ cargo check --all passes
□ cargo build passes
□ cargo build --release passes
```

### 2. Do all tests pass?
```
□ cargo test --all passes
□ No tests skipped with #[ignore] without reason
□ New code has new tests
```

### 3. No warnings?
```
□ cargo clippy --all -- -D warnings passes
□ No #[allow(dead_code)] without explanation
□ No #[allow(unused)] without explanation
□ No TODO comments without a tracking issue
```

### 4. Is the code documented?
```
□ Every public struct has a doc comment
□ Every public trait has a doc comment
□ Every public function has a doc comment
□ Every public enum variant has a comment
□ Complex logic has inline comments
```

### 5. Does it follow naming conventions?
```
□ All types PascalCase
□ All functions snake_case
□ All constants SCREAMING_SNAKE_CASE
□ No abbreviations except approved ones (ctx, id, fn)
```

### 6. Unsafe code?
```
□ Every unsafe block has a SAFETY comment
□ Unsafe blocks are minimal
□ No unnecessary unsafe
```

### 7. Does it stay in bounds?
```
□ Code is in the correct crate
□ No dependency going UP the hierarchy
□ Crate contract not violated
```

### 8. Does it emit traces?
```
□ New systems emit RosaceTrace events
□ Trace calls are behind #[cfg(debug_assertions)]
□ No trace overhead in release binary
```

### 9. Is the commit message good?
```
□ Format: type(scope): description
□ Types: feat, fix, refactor, test, docs, chore, perf
□ Scope: trace, core, state, layout, render, macros, cli
□ Description: present tense, lowercase, no period
□ Examples:
   feat(state): implement atom batching
   fix(layout): correct expanded in column constraint
   test(core): add error boundary panic tests
   docs(trace): document RosaceTrace variants
   perf(state): optimize refresh engine root finding
```

### 10. Would this pass a review?
```
□ A stranger could understand this code
□ No magic numbers without constants
□ No copy-pasted code (extract to function)
□ No commented-out code
□ No println! debug statements left in
```

---

## IF ANY BOX FAILS

Fix it. Do not commit until all boxes pass.
A bad commit is worse than a delayed commit.

---

## COMMIT MESSAGE EXAMPLES

```
Good:
feat(state): implement atom three-scope system
fix(layout): expanded widget respects bounded constraints
test(core): add lifecycle hook ordering tests
docs(trace): add RosaceTrace enum documentation
perf(render): reduce dirty region recalculation
chore(workspace): add rosace-scroll crate skeleton
refactor(core): extract child ordering into ChildContainer

Bad:
fix stuff
WIP
updated code
working now
```
