# psycopg2.connect() calls — expect psycopg2:connect binding
import psycopg2


def get_connection():
    """DSN string connection (positional arg)."""
    return psycopg2.connect("dbname=mydb user=postgres")


def get_connection_host():
    """Host-based DSN string."""
    return psycopg2.connect("host=localhost dbname=app user=admin password=secret")
