# Top-level shim for 'rusockslib' to make CI imports robust.
# This package directory at repo root may be discovered first on sys.path
# (as a namespace package or empty package). We extend __path__ to include
# the real package under _bindings/python/rusockslib and then import/re-export
# the expected submodule 'rusocks'.

from __future__ import annotations

import importlib
import os
import sys
from typing import Optional

# Ensure the real implementation path is on the package search path
_here = os.path.dirname(__file__)
_repo_root = os.path.dirname(_here)
_candidate = os.path.join(_repo_root, "_bindings", "python", "rusockslib")

# For regular packages, __path__ exists and controls submodule discovery
try:
    __path__  # type: ignore  # noqa: F401
except NameError:
    __path__ = []  # type: ignore

if os.path.isdir(_candidate) and _candidate not in __path__:  # type: ignore
    __path__.append(_candidate)  # type: ignore

# Try to import the submodule 'rusocks' from this package.
# This will resolve to _bindings/python/rusockslib/rusocks.[py|so|pyd] if present.
_rusocks_mod: Optional[object] = None
try:
    from . import rusocks as _rusocks_mod  # type: ignore
except Exception:
    # As a last resort, temporarily put the candidate on sys.path and try absolute import
    added = False
    if os.path.isdir(_candidate) and _candidate not in sys.path:
        sys.path.insert(0, _candidate)
        added = True
    try:
        _rusocks_mod = importlib.import_module("rusockslib.rusocks")
    except Exception:
        _rusocks_mod = None
    finally:
        if added:
            try:
                sys.path.remove(_candidate)
            except ValueError:
                pass

if _rusocks_mod is None:
    # If still unresolved, raise clear error explaining resolution path
    raise ImportError(
        "rusockslib.rusocks could not be imported. Expected to find it under "
        f"{_candidate}. Ensure the repository layout is intact or install a wheel."
    )

# Re-export as package attribute so 'from rusockslib import rusocks' works
rusocks = _rusocks_mod  # type: ignore

__all__ = ["rusocks"]