# Lightweight shim for importing the rusockslib.rusocks module in tests.
# If a built wheel is present and compatible, we attempt to import it.
# Otherwise, we fall back to a pure-Python stub in this package (rusocks.py).

from __future__ import annotations

import importlib
import os
import sys
import glob

_rusocks_mod = None  # type: ignore[var-annotated]

# Try to import compiled module from a wheel colocated with this package
try:
    _pkg_dir = os.path.dirname(__file__)
    wheel_candidates = sorted(glob.glob(os.path.join(_pkg_dir, "rusocks-*.whl")), reverse=True)

    for whl in wheel_candidates:
        # Temporarily add the wheel to sys.path to try import
        if whl not in sys.path:
            sys.path.insert(0, whl)
            _added = True
        else:
            _added = False

        try:
            _rusocks_mod = importlib.import_module("rusockslib.rusocks")
            break
        except Exception:
            # Not compatible or cannot import — continue to next candidate
            pass
        finally:
            # Leave path entry in place if it worked; otherwise remove what we added
            if _added and "rusockslib.rusocks" not in sys.modules and whl in sys.path:
                try:
                    sys.path.remove(whl)
                except ValueError:
                    pass
except Exception:
    # Ignore any probing errors and fall back to stub
    _rusocks_mod = None

# Fallback to local pure-Python stub
if _rusocks_mod is None:
    from . import rusocks as _rusocks_mod  # type: ignore

# Re-export as expected by "from rusockslib import rusocks"
rusocks = _rusocks_mod

__all__ = ["rusocks"]