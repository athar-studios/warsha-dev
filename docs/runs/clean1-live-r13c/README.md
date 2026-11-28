# CLEAN-1 live proof, round 13 (tip 35c7bf3)

Real OpenCode (`opencode/big-pickle`) ran the proofcalc spec as four parallel swarm lanes
through a real `warshad` (private HOME, fresh OpenCode database, `WARSHA_CODING_BACKEND=opencode`,
planning/review roles routed to the fake provider, gates approved programmatically).

Result: run reached `feedback completed`. All 4 subtasks `succeeded`, each persisted a
`swarm_coding_result` with `backend=opencode`, non-empty `files_changed` and `diff`:
`src/scaffold.rs`, `src/domain.rs`, `src/engine.rs`, `src/provider.rs`. Each file appeared
only in its own subtask worktree; the four branches merged onto the run branch without
conflicts; the integration checkout stayed clean. The merged modules were copied into a
scratch crate with a small test module: 3 tests passed (add, Money::checked_add overflow,
apply_fee 10000/250 -> Some(10250) and bps>10000 -> None).

Earlier runs of the same walk on 12e06b5 failed one lane in 0.5s because four simultaneous
`opencode run` processes raced to create OpenCode's SQLite schema on a fresh HOME
(`table account_state already exists`). Fixed by D-RN-OC-COLDSTART-1 (retry, 35c7bf3).

Limits: the lane output is only compile/unit checked in a scratch crate, not by a Warsha
review stage (review roles ran on the fake provider); one run, one model.
