// C-SB-1 test: SQLite C API
#include <sqlite3.h>

int db_open(void) {
    sqlite3* db;
    int rc = sqlite3_open("app.db", &db);
    if (rc == SQLITE_OK) {
        sqlite3_close(db);
    }
    return rc;
}

int db_open_v2(void) {
    sqlite3* db;
    int rc = sqlite3_open_v2("data.db", &db, SQLITE_OPEN_READWRITE, 0);
    if (rc == SQLITE_OK) {
        sqlite3_close(db);
    }
    return rc;
}
