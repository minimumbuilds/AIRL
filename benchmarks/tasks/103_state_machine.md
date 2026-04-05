Implement a connection state machine with states: Disconnected, Connecting, Connected, Error.
Transitions: connect(Disconnected->Connecting), established(Connecting->Connected), disconnect(Connected->Disconnected), fail(Connecting->Error), reset(Error->Disconnected).

Write a function `transition` that takes a state and event, returns the new state or an error if the transition is invalid.

Print the result of: Disconnected -> connect -> established -> disconnect
