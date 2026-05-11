# open() calls with read modes — expect open_read binding

def load_config():
    """Default mode (implicit 'r')."""
    f = open("/etc/config.yaml")
    return f.read()


def load_config_explicit():
    """Explicit 'r' mode."""
    f = open("/etc/config.yaml", "r")
    return f.read()


def load_binary():
    """Binary read mode 'rb'."""
    with open("/data/image.png", "rb") as f:
        return f.read()
