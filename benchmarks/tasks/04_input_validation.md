Write a function called `validate-age` that takes an integer and validates it as a human age.

Requirements:
- A valid age is between 0 and 150 (inclusive)
- If valid, return a success value containing the age
- If invalid, return an error value with a descriptive message
- The function must check bounds before returning

Test case: validate-age(25) should return a success with value 25
Test case: validate-age(-5) should return an error
Test case: validate-age(200) should return an error

Print the result of calling the function with argument 25.
