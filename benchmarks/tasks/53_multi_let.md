Write a function called `triangle-area` that takes three side lengths and returns the area using Heron's formula, or -1 if the sides cannot form a valid triangle.

Requirements:
- First check the triangle inequality: each side must be less than the sum of the other two
- Compute s = (a + b + c) / 2 (use float arithmetic)
- Compute area = sqrt(s * (s-a) * (s-b) * (s-c))
- Return the area truncated to an integer, or -1 for invalid triangles
- Use multi-binding let to bind intermediate values

Test case: triangle-area(3, 4, 5) should return 6
Test case: triangle-area(1, 1, 10) should return -1

Print the result of calling the function with arguments 5, 12, and 13.
