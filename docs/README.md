# ROSACE — Steering Files
> Fast by nature. Beautiful by design.

These files are the single source of truth for the ROSACE project.
Every decision, every step, every verification checkpoint lives here.
Nothing gets built without a corresponding steering file.
Nothing gets merged without passing its checklist.

---

## FILE INDEX

```
README.md                    ← this file, index of all steering files
DECISIONS.md                 ← every architecture decision, locked
PRINCIPLES.md                ← non-negotiable rules, never broken
PHASE_1.md                   ← Phase 1 detailed plan + checklists
PHASE_2.md                   ← Phase 2 plan (locked after Phase 1)
PHASE_3.md                   ← Phase 3 plan (locked after Phase 2)
CRATE_CONTRACTS.md           ← what each crate does, its boundaries
NAMING.md                    ← naming conventions, enforced everywhere
CHECKLIST_BEFORE_CODE.md     ← run before writing any new code
CHECKLIST_BEFORE_COMMIT.md   ← run before every git commit
CHECKLIST_BEFORE_MERGE.md    ← run before merging any PR
ERROR_CATALOGUE.md           ← all T-series compiler errors
SKILLS.md                    ← how to work with Claude on this project
GLOSSARY.md                  ← every term defined precisely
```

---

## HOW TO USE THESE FILES

### Starting a new session
1. Read PRINCIPLES.md — remind yourself what cannot change
2. Read the current phase file — know exactly where you are
3. Read CHECKLIST_BEFORE_CODE.md — before writing anything

### Making a decision
1. Check DECISIONS.md — is it already decided?
2. If yes — follow it, do not re-debate
3. If no — discuss, decide, document in DECISIONS.md immediately

### Writing code
1. Run CHECKLIST_BEFORE_CODE.md
2. Check CRATE_CONTRACTS.md — stay within crate boundaries
3. Check NAMING.md — use correct names
4. Write code
5. Run CHECKLIST_BEFORE_COMMIT.md
6. Commit

### Working with Claude
1. Read SKILLS.md — how to get the best help
2. Always paste relevant steering files into context
3. Always verify Claude output against DECISIONS.md

---

## GOLDEN RULE

> If it is not in a steering file, it does not exist.
> If it contradicts a steering file, it is wrong.
> If a steering file needs updating, update it before writing code.
