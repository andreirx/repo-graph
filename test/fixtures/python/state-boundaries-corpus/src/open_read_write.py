# open() calls with read-write modes — expect open_read_write binding

def update_config():
    """Read-write mode 'r+'."""
    with open("/etc/config.yaml", "r+") as f:
        content = f.read()
        f.seek(0)
        f.write(content.upper())


def truncate_and_write():
    """Write-read mode 'w+'."""
    with open("/tmp/scratch.txt", "w+") as f:
        f.write("data")
        f.seek(0)
        return f.read()


def append_and_read():
    """Append-read mode 'a+'."""
    with open("/var/log/app.log", "a+") as f:
        f.write("new entry")
        f.seek(0)
        return f.read()


def binary_read_write():
    """Binary read-write mode 'r+b'."""
    with open("/data/file.bin", "r+b") as f:
        header = f.read(4)
        f.seek(0)
        f.write(b"\xff" + header[1:])
