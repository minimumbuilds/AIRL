Write a function called `has-path` that checks whether a path exists between two nodes in a directed graph.

Requirements:
- The graph is represented as a map where each key is a node name (string) and the value is a list of neighbor node names
- Use depth-first search with a visited set to avoid cycles
- Return true if a path exists from start to end, false otherwise
- A node always has a path to itself

Test case: has-path({"A": ["B", "C"], "B": ["D"], "C": ["D"], "D": []}, "A", "D") should return true
Test case: has-path({"A": ["B"], "B": [], "C": []}, "A", "C") should return false

Print the result of calling the function with a graph {"a": ["b", "c"], "b": ["d"], "c": [], "d": ["e"], "e": []}, start "a", end "e".
