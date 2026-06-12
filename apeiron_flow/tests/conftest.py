"""
pytest conftest — session-wide fixtures and environment bootstrapping.

main.py instantiates SQLiteFlowPersistence(db_path=RESPOND_DB) at module
level (as a @persist decorator argument). If ~/.config/apeiron-flow/ does
not exist the import fails with sqlite3.OperationalError.

We must set RESPOND_DB before any test module triggers the import of
apeiron_flow.main (or apeiron_flow.config), so this conftest sets the env
var at module load time, which pytest guarantees happens before collection.
"""

import os
import tempfile

_tmp_db = tempfile.NamedTemporaryFile(suffix=".db", delete=False)
_tmp_db.close()
os.environ.setdefault("RESPOND_DB", _tmp_db.name)
