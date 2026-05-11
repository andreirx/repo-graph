# sqlite3.connect() calls — expect sqlite3:connect binding
import sqlite3


def get_connection():
    """Standard sqlite3 connection with literal path."""
    return sqlite3.connect("app.db")


def get_memory_db():
    """In-memory database (special :memory: path)."""
    return sqlite3.connect(":memory:")


def get_abs_path_db():
    """Absolute path database."""
    return sqlite3.connect("/var/data/production.db")
