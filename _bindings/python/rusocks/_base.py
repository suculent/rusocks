"""
Base classes and utilities for rusocks.

This module contains shared functionality used by Server and Client classes.
"""

from __future__ import annotations

import json
import logging
import asyncio
import threading
import time
from dataclasses import dataclass
from datetime import timedelta
from typing import Any, Callable, Dict, Iterable, Optional, Tuple, Union, List

# Underlying Rust bindings module (generated)
from rusockslib import rusocks # type: ignore

_logger = logging.getLogger(__name__)

# Type aliases
DurationLike = Union[int, float, timedelta, str]


def _snake_to_camel(name: str) -> str:
    """Convert snake_case to CamelCase."""
    parts = name.split("_")
    return "".join(p.capitalize() for p in parts if p)


def _camel_to_snake(name: str) -> str:
    """Convert CamelCase to snake_case."""
    out: List[str] = []
    for ch in name:
        if ch.isupper() and out:
            out.append("_")
        out.append(ch.lower())
    return "".join(out)


def _to_duration(value: Optional[DurationLike]) -> Any:
    """Convert seconds/str/timedelta to Rust time.Duration via bindings.
    
    - None -> 0
    - int/float -> seconds (supports fractions)
    - timedelta -> total seconds
    - str -> parsed by Rust (e.g., "1.5s", "300ms")
    """
    if value is None:
        return 0
    if isinstance(value, timedelta):
        seconds = value.total_seconds()
        return seconds * rusocks.SECOND
    if isinstance(value, (int, float)):
        return value * rusocks.SECOND
    if isinstance(value, str):
        try:
            return rusocks.parse_duration(value)
        except Exception as exc:
            raise ValueError(f"Invalid duration string: {value}") from exc
    raise TypeError(f"Unsupported duration type: {type(value)!r}")


# Shared Rust->Python log dispatcher
_def_level_map = {
    "trace": logging.DEBUG,
    "debug": logging.DEBUG,
    "info": logging.INFO,
    "warn": logging.WARNING,
    "warning": logging.WARNING,
    "error": logging.ERROR,
    "fatal": logging.CRITICAL,
    "panic": logging.CRITICAL,
}


def _emit_rust_log(py_logger: logging.Logger, line: str) -> None:
    """Process a Rust log line and emit it to the Python logger."""
    try:
        obj = json.loads(line)
    except Exception:
        py_logger.info(line)
        return
    level = str(obj.get("level", "")).lower()
    message = obj.get("message") or obj.get("msg") or ""
    extras: Dict[str, Any] = {}
    for k, v in obj.items():
        if k in ("level", "time", "message", "msg"):
            continue
        extras[k] = v
    py_logger.log(_def_level_map.get(level, logging.INFO), message, extra={"rust": extras})


# Global registry for logger instances
_logger_registry: Dict[str, logging.Logger] = {}

# Event-driven log monitoring system
_log_listeners: List[Callable[[List], None]] = []
_listener_thread: Optional[threading.Thread] = None
_listener_active: bool = False


def _start_log_listener() -> None:
    """Start background thread to drain Rust log buffer and forward to Python loggers."""
    global _listener_thread, _listener_active
    if _listener_active and _listener_thread and _listener_thread.is_alive():
        return
    _listener_active = True

    def _run() -> None:
        # Drain loop: wait for entries with timeout to allow graceful shutdown
        while _listener_active:
            try:
                entries = rusocks.wait_for_log_entries(2000)  # wait up to 2s
            except Exception:
                # Backoff on unexpected errors to avoid busy loop
                time.sleep(0.2)
                continue

            if not entries:
                continue

            # Iterate returned entries; handle both attr and dict styles
            for entry in entries:
                try:
                    logger_id = getattr(entry, "logger_id", None)
                    if logger_id is None and isinstance(entry, dict):
                        logger_id = entry.get("logger_id")

                    message = getattr(entry, "message", None)
                    if message is None and isinstance(entry, dict):
                        message = entry.get("message")

                    if not message:
                        continue

                    py_logger = _logger_registry.get(str(logger_id)) or _logger
                    _emit_rust_log(py_logger, str(message))
                except Exception:
                    # Never let logging path crash the listener
                    continue

    _listener_thread = threading.Thread(target=_run, name="rusocks-rust-log-listener", daemon=True)
    _listener_thread.start()


def _stop_log_listener() -> None:
    """Stop the background log listener thread."""
    global _listener_active
    _listener_active = False
    try:
        # Unblock wait_for_log_entries callers
        rusocks.cancel_log_waiters()
    except Exception:
        pass


class BufferZerologLogger:
    """Buffer-based logger system for Rust bindings."""
    
    def __init__(self, py_logger: logging.Logger, logger_id: str):
        self.py_logger = py_logger
        self.logger_id = logger_id
        # Ensure background listener is running
        _start_log_listener()

        # Create a new Rust logger with ID
        self.rust_logger = rusocks.PythonLogger.new(self.logger_id)
        _logger_registry[logger_id] = py_logger
    
    def cleanup(self):
        """Clean up logger resources."""
        if self.logger_id in _logger_registry:
            del _logger_registry[self.logger_id]


@dataclass
class ReverseTokenResult:
    """Result of adding a reverse token."""
    token: str
    port: int


class _SnakePassthrough:
    """Mixin to map snake_case attribute access to underlying CamelCase.
    
    Only used when an explicit Pythonic method/attribute is not defined.
    """

    def __getattr__(self, name: str) -> Any:
        raw = super().__getattribute__("_raw")  # type: ignore[attr-defined]
        camel = _snake_to_camel(name)
        try:
            return getattr(raw, camel)
        except AttributeError:
            raise

    def __dir__(self) -> List[str]:
        # Expose snake_case versions of underlying CamelCase for IDEs
        names = set(super().__dir__())
        try:
            raw = super().__getattribute__("_raw")  # type: ignore[attr-defined]
            for attr in dir(raw):
                if not attr or attr.startswith("_"):
                    continue
                names.add(_camel_to_snake(attr))
        except Exception:
            pass
        return sorted(names)


def set_log_level(level: Union[int, str]) -> None:
    """Set the global log level for rusocks."""
    if isinstance(level, str):
        level = getattr(logging, level.upper())
    _logger.setLevel(level)
    
    # Also set the Rust logger level
    level_name = logging.getLevelName(level)
    if level_name == "DEBUG":
        rusocks.set_logger_global_level(rusocks.Level.Debug)
    elif level_name == "INFO":
        rusocks.set_logger_global_level(rusocks.Level.Info)
    elif level_name == "WARNING":
        rusocks.set_logger_global_level(rusocks.Level.Warn)
    elif level_name == "ERROR":
        rusocks.set_logger_global_level(rusocks.Level.Error)
    elif level_name == "CRITICAL":
        rusocks.set_logger_global_level(rusocks.Level.Error)
    else:
        # Default to INFO
        rusocks.set_logger_global_level(rusocks.Level.Info)