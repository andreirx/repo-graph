# open() calls with write modes — expect open_write binding

def save_log():
    """Write mode 'w'."""
    with open("/var/log/app.log", "w") as f:
        f.write("started")


def append_log():
    """Append mode 'a'."""
    with open("/var/log/app.log", "a") as f:
        f.write("entry")


def create_exclusive():
    """Exclusive create mode 'x'."""
    with open("/tmp/lockfile", "x") as f:
        f.write("locked")


def write_binary():
    """Binary write mode 'wb'."""
    with open("/data/output.bin", "wb") as f:
        f.write(b"\x00\x01\x02")
