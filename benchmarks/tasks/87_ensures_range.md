Write a function called `percentage` that takes a numerator and denominator and returns the percentage as an integer (0-100).

Requirements:
- The :ensures contract must guarantee result is between 0 and 100 inclusive
- Use integer arithmetic: (numerator * 100) / denominator
- The denominator must be positive (enforce in :requires)
- The numerator must be between 0 and denominator inclusive (enforce in :requires)

Test case: percentage(3, 4) should return 75
Test case: percentage(1, 3) should return 33
Test case: percentage(0, 5) should return 0

Print the result of calling the function with arguments 7 and 10.
