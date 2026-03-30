Write a function called `center-pad` that takes a string and a target width, and returns the string centered with spaces on both sides.

Requirements:
- If the string is already at or beyond the target width, return it unchanged
- Add spaces evenly on both sides to reach the target width
- If an odd number of spaces is needed, put the extra space on the right
- The result length should equal the target width (unless input is longer)

Test case: center-pad("hi", 6) should return "  hi  "
Test case: center-pad("hello", 3) should return "hello"

Print the result of calling the function with arguments "hi" and 8.
