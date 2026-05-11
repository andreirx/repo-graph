"""Nested functions fixture for PY-EXT-2 validation."""

_module_state = []


def outer_function(x: int):
    """Function with nested definitions and closures."""
    captured = x * 2

    def inner_function(y: int) -> int:
        """Inner function that captures outer scope."""
        local_var = y + captured
        return local_var

    def another_inner():
        """Another nested function."""
        inner_result = 0
        for i in range(5):
            inner_result += i
        return inner_result

    result = inner_function(10)
    return result + another_inner()


def function_with_lambda():
    """Function using lambdas."""
    multiplier = 2
    items = [1, 2, 3, 4, 5]
    doubled = list(map(lambda x: x * multiplier, items))
    return doubled


class ClassWithNestedDef:
    """Class containing methods with nested functions."""

    def method_with_closure(self, factor: int):
        """Method that defines a closure."""
        base_value = 100

        def compute(x: int) -> int:
            inner_temp = x * factor
            return inner_temp + base_value

        return compute
