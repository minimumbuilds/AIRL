Write a simple state machine that processes a list of commands and tracks state transitions.

Requirements:
- States are strings: "idle", "running", "paused", "stopped"
- Commands are strings: "start", "pause", "resume", "stop"
- Transitions: idle+start->running, running+pause->paused, paused+resume->running, running+stop->stopped, paused+stop->stopped
- Invalid transitions leave the state unchanged
- Return the final state after processing all commands

Write a function called `run-machine` that takes an initial state and a list of commands, returning the final state.

Test case: run-machine("idle", ["start", "pause", "resume", "stop"]) should return "stopped"
Test case: run-machine("idle", ["pause"]) should return "idle" (invalid transition)

Print the result of calling the function with "idle" and ["start", "pause", "resume", "pause", "stop"].
