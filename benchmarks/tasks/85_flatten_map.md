Write a function called `flatten-keys` that takes a nested map and returns a flat map with dotted key paths.

Requirements:
- Nested keys are joined with "."
- Only leaf values (non-map values) appear in the result
- Top-level keys with non-map values are kept as-is
- An empty map returns an empty map

Test case: flatten-keys({"a": {"b": 1, "c": 2}}) should return {"a.b": 1, "a.c": 2}
Test case: flatten-keys({"x": 10}) should return {"x": 10}

Print the result of calling the function with a map {"name": "AIRL", "version": {"major": 0, "minor": 6}}.
