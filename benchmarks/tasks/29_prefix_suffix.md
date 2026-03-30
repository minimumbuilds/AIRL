Write a function called `classify-filename` that takes a filename string and returns a string describing its type based on extension.

Requirements:
- If the filename ends with ".airl", return "airl"
- If the filename ends with ".py", return "python"
- If the filename ends with ".md", return "markdown"
- Otherwise return "unknown"
- Use starts-with or ends-with for checking

Test case: classify-filename("hello.airl") should return "airl"
Test case: classify-filename("README.md") should return "markdown"
Test case: classify-filename("data.csv") should return "unknown"

Print the result of calling the function with argument "test.py".
