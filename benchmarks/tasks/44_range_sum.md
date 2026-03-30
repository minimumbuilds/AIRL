Write a function called `range-sum` that takes two integers `start` and `end` and returns the sum of all integers from start (inclusive) to end (exclusive).

Requirements:
- Use range to generate the sequence
- If start >= end, return 0
- The result should be the sum of [start, start+1, ..., end-1]

Test case: range-sum(1, 6) should return 15 (1+2+3+4+5)
Test case: range-sum(5, 5) should return 0
Test case: range-sum(10, 13) should return 33 (10+11+12)

Print the result of calling the function with arguments 1 and 11.
