"""Simple class fixture for PY-EXT-2 validation."""

# Module-level variable
CONFIG_VALUE = "default"

# Annotated assignment (PY-EXT-2 requirement)
DEBUG_MODE: bool = False


class MyClass:
    """A simple class with constructor and methods."""

    class_variable = 0

    def __init__(self, value: int):
        """Initialize with a value."""
        self.value = value
        self._private = None

    def get_value(self) -> int:
        """Return the stored value."""
        return self.value

    def set_value(self, new_value: int) -> None:
        """Set a new value."""
        self.value = new_value

    def _internal_method(self):
        """Private method."""
        count = 0
        for i in range(10):
            count += i
        return count


def standalone_function(x: int, y: int) -> int:
    """A standalone function with local variables."""
    result = x + y
    temp = result * 2
    # Annotated local assignment (PY-EXT-2 requirement)
    final: int = temp + 1
    return final
