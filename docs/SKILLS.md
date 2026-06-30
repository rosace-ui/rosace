# TEZZERA — SKILLS
> How to work effectively with Claude on this project.
> Follow these patterns to get the best results.
> Mistakes here cost real development time.

---

## BEFORE EVERY SESSION

Always paste these into the conversation:
1. The relevant section of DECISIONS.md
2. The relevant CRATE_CONTRACTS.md section
3. The current phase file (PHASE_1.md etc.)
4. Any code you are working on

Without this context, Claude may give advice that
contradicts decisions already made.

---

## SKILL 1 — IMPLEMENTING A SYSTEM

When asking Claude to implement something:

**Good prompt:**
```
I am implementing the Atom<T> struct for tezzera-state.

Relevant decisions:
[paste D006, D007 from DECISIONS.md]

Crate contract:
[paste tezzera-state section from CRATE_CONTRACTS.md]

Current phase: Phase 1
Current step: Step 4a (basic Atom<T>)

Please implement Atom<T> with:
- get() method
- set() method  
- update() method
- subscriber notification on change
- TezzeraTrace emission on read and write

Show me the implementation with tests.
```

**Bad prompt:**
```
implement the atom system
```

---

## SKILL 2 — VERIFYING A DECISION

When unsure if something is already decided:

**Good prompt:**
```
I want to add a fourth atom scope for module-level atoms.
Before I proceed, please check if this conflicts with
any decisions in DECISIONS.md:

[paste relevant DECISIONS.md sections]

If it conflicts, tell me which decision and why.
If it does not conflict, help me write the decision
entry before we write any code.
```

**Rule:** Never implement before the decision is documented.

---

## SKILL 3 — DEBUGGING

When something is not working:

**Good prompt:**
```
I am in tezzera-state, Step 4d (RefreshEngine).
The refresh engine is rebuilding child components
even when their parent is already in the dirty set.

Here is the current implementation:
[paste code]

Here is the failing test:
[paste test]

Here is the expected behavior from DECISIONS.md D011:
[paste decision]

What is wrong and how do I fix it?
```

**Always include:**
- Which crate and step
- The actual code
- The failing test
- The expected behavior from decisions

---

## SKILL 4 — DESIGN QUESTIONS

When facing a design choice not yet decided:

**Good prompt:**
```
I am designing the glyph cache for text rendering
(tezzera-layout, Step 5h).

The relevant decision is D019 (Text Layout):
[paste D019]

I have two options for the GPU texture atlas:
Option A: Fixed size atlas, reject glyphs when full
Option B: Dynamic atlas, grows as needed, LRU eviction

Which aligns better with our principles and decisions?
Please check against PRINCIPLES.md:
[paste relevant principles]
```

---

## SKILL 5 — CODE REVIEW

When asking Claude to review code:

**Good prompt:**
```
Please review this implementation of the refresh engine
against our decisions and principles.

Decision D011 says:
[paste decision]

Principles P2 (efficiency) and P5 (compile time) are relevant.

Here is the implementation:
[paste code]

Check for:
1. Correctness against the decision
2. Performance issues
3. Missing trace emissions
4. Missing tests
5. Naming convention violations
6. Any crate boundary violations
```

---

## SKILL 6 — WRITING TESTS

When writing tests:

**Good prompt:**
```
I need tests for the Atom batching system (D010).

The decision says:
[paste D010]

The implementation is:
[paste code]

Please write tests that cover:
1. Automatic batching within sync blocks
2. Manual batch() API
3. Priority levels
4. That only ONE rebuild happens for multiple atom changes
5. Edge cases from the decision
```

---

## SKILL 7 — ADDING A DEPENDENCY

Before adding any new crate dependency:

**Good prompt:**
```
I need to add a dependency for [purpose].
I am considering [crate-name] version [x.y.z].

Please check:
1. Is it already in the approved dependencies list in PHASE_1.md?
2. Does it conflict with any existing dependency?
3. Does adding it violate any crate contract in CRATE_CONTRACTS.md?
4. What is the license? (must be MIT or Apache-2.0 compatible)
5. Is it actively maintained?
6. What is the binary size impact?

If approved, I will add it to the approved list in PHASE_1.md.
```

---

## SKILL 8 — NAMING SOMETHING NEW

When naming a new type, function, or file:

**Good prompt:**
```
I need to name the struct that holds the DFS timestamps
for O(1) ancestor lookup in the refresh engine.

NAMING.md says types are PascalCase and should be descriptive.

Candidates I am considering:
- TreeIndex
- AncestorIndex  
- DfsIndex
- ComponentTreeIndex

Which best follows our conventions and is clearest?
```

---

## WHAT CLAUDE WILL ALWAYS DO

When working on TEZZERA, Claude will:
- Check decisions before suggesting implementations
- Flag if a suggestion contradicts DECISIONS.md
- Suggest adding a decision before writing code
- Follow NAMING.md conventions exactly
- Include trace emissions in all system implementations
- Write tests alongside implementations
- Stay within crate boundaries
- Flag if a dependency needs approval

---

## WHAT TO DO IF CLAUDE MAKES A MISTAKE

If Claude suggests something that contradicts a decision:

1. Point to the specific decision: "D011 says X, but you suggested Y"
2. Ask Claude to revise within the constraint
3. Do not implement the contradicting suggestion
4. If the decision seems wrong — update DECISIONS.md first, then implement

---

## SESSION STARTER TEMPLATE

Copy this at the start of every working session:

```
I am working on TEZZERA, a Rust UI framework.
Fast by nature. Beautiful by design.

Current phase: Phase 1 — Foundation
Current step: [STEP NUMBER AND NAME]
Current crate: [CRATE NAME]

Key decisions for this session:
[paste relevant decisions from DECISIONS.md]

Crate contract:
[paste relevant section from CRATE_CONTRACTS.md]

Today's goal:
[what you want to accomplish]

Please help me implement this while:
- Following all decisions in DECISIONS.md
- Staying within the crate contract
- Using names from NAMING.md
- Including TezzeraTrace emissions
- Writing tests
```
