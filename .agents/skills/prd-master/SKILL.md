---
name: prd-master
description: Product requirements and PRD authoring skill for writing or reviewing PRDs, user stories, acceptance criteria, scope boundaries, and feature prioritization. Use when defining product scope, translating business goals into requirements, or preparing handoff-ready specs for design and engineering.
---

# PRD Master

Create decision-ready product requirements that are testable, measurable, and implementation-ready.

## Hard Rules

- Quantify goals and constraints. Avoid vague language like "fast", "easy", or "scalable" without targets.
- Describe the problem before proposing the solution.
- Keep scope explicit with `In scope` and `Out of scope`.
- Require testable acceptance criteria for every core story.
- Separate requirements from implementation details unless technical constraints require specificity.

## Execution Workflow

1. Clarify context: product goal, users, timeline, constraints, dependencies.
2. Define problem: current pain, impact, and why now.
3. Define outcomes: measurable success metrics with baseline, target, and timeline.
4. Define scope: prioritize capabilities and document exclusions.
5. Specify requirements: functional, non-functional, edge cases, and acceptance criteria.
6. Prepare rollout: launch strategy, monitoring, risks, and open questions.

## Output Contract

When producing a PRD, return these sections in order:

1. Executive summary
2. Problem and context
3. Goals and success metrics
4. Target users/personas
5. Scope (`In scope` / `Out of scope`)
6. User stories
7. Acceptance criteria
8. Functional requirements
9. Non-functional requirements
10. Rollout and measurement plan
11. Risks, assumptions, and open questions

For each section, prefer concise bullets, tables, and checklists over long prose.

## Quality Gate

Before finalizing, verify:

- Every key metric has numbers and timeframe.
- Every high-priority story follows user-value framing.
- Every high-priority story has acceptance criteria.
- Performance/security/compliance requirements are stated where relevant.
- Risks and unresolved questions are explicit with owners.

## Progressive Loading Guide

Load references only when needed:

- Story writing patterns and INVEST checks:
  [references/user-stories.md](references/user-stories.md)
- Feature prioritization frameworks (RICE, ICE, MoSCoW, Kano):
  [references/prioritization.md](references/prioritization.md)
- BDD and acceptance criteria patterns:
  [references/acceptance-criteria.md](references/acceptance-criteria.md)
- Full PRD scaffold for direct drafting:
  [templates/prd-template.md](templates/prd-template.md)

## Default Working Mode

- If user provides partial input, draft a complete PRD with explicit assumptions.
- If user asks for review, return issues first, ordered by severity, with concrete fixes.
- If data is missing for metrics, provide suggested targets and mark them as assumptions.
