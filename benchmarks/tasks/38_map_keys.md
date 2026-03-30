Write a function called `invert-map` that takes a map and returns a new map with keys and values swapped.

Requirements:
- Each value in the input becomes a key in the output, and vice versa
- All values in the input map must be strings (since map keys must be strings)
- If multiple keys map to the same value, the last one wins

Test case: invert-map({"a": "1", "b": "2", "c": "3"}) should return {"1": "a", "2": "b", "3": "c"}

Print the result of calling the function with a map created from ["x" "10" "y" "20" "z" "30"].
