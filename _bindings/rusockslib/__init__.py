# CI/path shim: ensure "from rusockslib import rusocks" works when tests prepend _bindings to sys.path.
# We extend this package's search path to include the real package under _bindings/python/rusockslib,
# then import and re-export the "rusocks" submodule.

from __future__ import annotations

import importlib
import os
import sys
from typing import Optional

_here = os.path.dirname(__file__)
_bindings_dir = os.path.dirname(_here)
_candidate = os.path.join(_bindings_dir, "python", "rusockslib")

# Ensure this package can find submodules under the real impl directory
try:
    __path__  # type: ignore  # noqa: F401
except NameError:
    __path__ = []  # type: ignore

if os.path.isdir(_candidate) and _candidate not in __path__:  # type: ignore
    __path__.append(_candidate)  # type: ignore

_rusocks_mod: Optional[object] = None

try:
    # Prefer package-relative import: resolves to _bindings/python/rusockslib/rusocks.py
    from . import rusocks as _rusocks_mod  # type: ignore
except Exception:
    # Fallback: temporarily add the concrete dir to sys.path and try absolute import
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
    raise ImportError(
        "rusockslib.rusocks could not be imported. Expected at: "
        f"{_candidate}. Ensure the repo layout is intact or install a wheel."
    )

rusocks = _rusocks_mod  # type: ignore

__all__ = ["rusocks"]