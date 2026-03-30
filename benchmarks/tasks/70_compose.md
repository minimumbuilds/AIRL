Write a function called `compose` that takes two single-argument functions and returns a new function that applies them in sequence (f after g).

Requirements:
- compose(f, g) returns a function h where h(x) = f(g(x))
- The returned function should work when called with any argument
- This is standard mathematical function composition

Demonstrate by composing a doubling function (fn [x] (* x 2)) with an increment function (fn [x] (+ x 1)), then calling the result with 5. The composed function should compute (* (+ 5 1) 2) = 12 (increment first, then double).

Print the result of calling the composed function with argument 5.
