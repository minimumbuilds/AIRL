Write a function called `rle-encode` that performs run-length encoding on a list.

Requirements:
- Consecutive identical elements are grouped into [value, count] pairs
- Return a list of [value, count] pairs
- Single-element runs have count 1
- An empty list returns an empty list

Test case: rle-encode([1, 1, 2, 3, 3, 3, 1]) should return [[1, 2], [2, 1], [3, 3], [1, 1]]
Test case: rle-encode(["a", "a", "b", "b", "b"]) should return [["a", 2], ["b", 3]]

Print the result of calling the function with argument [1, 1, 1, 2, 2, 3, 1, 1].
