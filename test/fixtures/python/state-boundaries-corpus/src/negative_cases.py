# Negative cases — patterns NOT expected to produce state boundary edges
# These test the boundaries of what the extractor/adapter can handle.
import os
import sqlite3
from pathlib import Path


# === DEFERRED: pathlib.Path methods (needs receiver_payload) ===

def pathlib_read():
    """pathlib.Path.read_text() — resource is on receiver, not arg0."""
    p = Path("/etc/config.yaml")
    return p.read_text()


def pathlib_write():
    """pathlib.Path.write_text() — resource is on receiver, not arg0."""
    p = Path("/var/log/app.log")
    p.write_text("data")


# === DEFERRED: keyword-arg DB constructors ===

def psycopg2_kwargs():
    """psycopg2.connect(host=..., dbname=...) — needs keyword arg payload."""
    import psycopg2
    return psycopg2.connect(host="localhost", dbname="app", user="admin")


def mysql_kwargs():
    """mysql.connector.connect(**kwargs) — needs keyword arg payload."""
    import mysql.connector
    return mysql.connector.connect(host="localhost", database="app")


# === DEFERRED: cursor.execute (needs provenance tracking) ===

def sqlite_execute():
    """cursor.execute() — needs connection→cursor provenance."""
    conn = sqlite3.connect("app.db")
    cur = conn.cursor()
    cur.execute("SELECT * FROM users")
    return cur.fetchall()


# === NOT EXTRACTABLE: dynamic paths ===

def dynamic_path():
    """Variable path — not a string literal, cannot extract."""
    config_path = os.environ.get("CONFIG_PATH", "/etc/default.yaml")
    return open(config_path)


def computed_path():
    """Computed path — f-string, cannot extract as literal."""
    name = "app"
    return open(f"/etc/{name}.yaml")
