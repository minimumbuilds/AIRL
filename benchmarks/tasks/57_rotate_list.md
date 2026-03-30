Write a function called `rotate-left` that takes a list and an integer N, and rotates the list N positions to the left.

Requirements:
- Elements shifted off the left end wrap around to the right
- N can be larger than the list length (use modulo)
- N of 0 returns the list unchanged
- An empty list returns an empty list

Test case: rotate-left([1, 2, 3, 4, 5], 2) should return [3, 4, 5, 1, 2]
Test case: rotate-left([1, 2, 3], 5) should return [3, 1, 2] (5 mod 3 = 2)

Print the result of calling the function with arguments [10, 20, 30, 40, 50] and 3.
