from __future__ import annotations

import json
import threading
import time
import uuid
from dataclasses import dataclass
from enum import Enum
from typing import Any, Dict, List, Optional


# ---- Time constants (nanoseconds) ----
NANOSECOND = 1
MICROSECOND = 1_000
MILLISECOND = 1_000_000
SECOND = 1_000_000_000
MINUTE = 60 * SECOND
HOUR = 3600 * SECOND


# ---- Global runtime shims ----
def init_global_runtime() -> None:
    # No-op for pure-Python shim
    return None


def cancel_global_runtime() -> None:
    # No-op for pure-Python shim
    return None


# ---- Log infrastructure compatible with _bindings/python/rusocks/_base.py ----
_LOG_ENTRIES: List[Dict[str, Any]] = []
_LOG_COND = threading.Condition()


def _push_log(logger_id: str, level: str, message: str) -> None:
    """
    Append a log entry into the shared queue. The 'message' field must be a JSON string
    that includes "level" and "message" keys to match _emit_rust_log() expectations.
    """
    # Serialize the payload the same way the Rust shim does
    payload = json.dumps({"level": level, "message": message})
    entry = {"logger_id": logger_id, "message": payload, "time": int(time.time_ns())}
    with _LOG_COND:
        _LOG_ENTRIES.append(entry)
        _LOG_COND.notify_all()


def wait_for_log_entries(timeout_ms: int) -> List[Dict[str, Any]]:
    """
    Block up to timeout_ms milliseconds for log entries and return a batch.
    Returns [] on timeout, otherwise returns and clears the current buffer.
    """
    deadline = time.time() + (timeout_ms / 1000.0) if timeout_ms and timeout_ms > 0 else None
    with _LOG_COND:
        # If already have entries, return immediately
        if _LOG_ENTRIES:
            batch = list(_LOG_ENTRIES)
            _LOG_ENTRIES.clear()
            return batch

        # Otherwise, wait
        while True:
            if deadline is None:
                _LOG_COND.wait()
            else:
                remaining = deadline - time.time()
                if remaining <= 0:
                    return []
                _LOG_COND.wait(timeout=remaining)

            if _LOG_ENTRIES:
                batch = list(_LOG_ENTRIES)
                _LOG_ENTRIES.clear()
                return batch


def cancel_log_waiters() -> None:
    """
    Wake all waiters (used by _stop_log_listener in Python layer).
    """
    with _LOG_COND:
        _LOG_COND.notify_all()


# ---- Log level control ----
class Level(Enum):
    Error = "error"
    Warn = "warn"
    Info = "info"
    Debug = "debug"
    Trace = "trace"


# Track a global level (defaults to Info)
_global_level: Level = Level.Info

_level_order = {
    Level.Error: 40,
    Level.Warn: 30,
    Level.Info: 20,
    Level.Debug: 10,
    Level.Trace: 5,
}


def set_logger_global_level(level: Level) -> None:
    global _global_level
    _global_level = level


# ---- PythonLogger shim ----
class PythonLogger:
    def __init__(self, logger_id: str, level: Optional[Level] = None):
        self._id = logger_id
        self._level = level or Level.Info

    @staticmethod
    def new(logger_id: str) -> "PythonLogger":
        return PythonLogger(logger_id)

    @staticmethod
    def new_with_level(logger_id: str, level: Level) -> "PythonLogger":
        return PythonLogger(logger_id, level)

    def set_level(self, level: Level) -> None:
        self._level = level

    def _enabled(self, level: Level) -> bool:
        return _level_order[level] >= _level_order.get(self._level, 20)

    def trace(self, message: str) -> None:
        if self._enabled(Level.Trace):
            _push_log(self._id, Level.Trace.value, message)

    def debug(self, message: str) -> None:
        if self._enabled(Level.Debug):
            _push_log(self._id, Level.Debug.value, message)

    def info(self, message: str) -> None:
        if self._enabled(Level.Info):
            _push_log(self._id, Level.Info.value, message)

    def warn(self, message: str) -> None:
        if self._enabled(Level.Warn):
            _push_log(self._id, Level.Warn.value, message)

    def error(self, message: str) -> None:
        if self._enabled(Level.Error):
            _push_log(self._id, Level.Error.value, message)


# ---- Duration parsing ----
def parse_duration(s: str) -> int:
    """
    Parse a duration string similar to the Rust implementation and return nanoseconds (int).
    Supports:
      - ns, us (or µs), ms, s, m, h
      - floats like "1.5s"
      - composite values like "1.5s" + "500ms" are not supported here; single segment only.
    """
    s = s.strip()
    if not s:
        raise ValueError("Empty duration string")

    # Extract numeric part (float) and unit suffix
    # Find first alpha char index
    idx = 0
    while idx < len(s) and (s[idx].isdigit() or s[idx] in "+-."):
        idx += 1
    num_str = s[:idx].strip()
    unit_str: str = s[idx:].strip().lower()

    if not num_str or not unit_str:
        raise ValueError(f"Invalid duration format: {s!r}")

    val = float(num_str)

    if unit_str in ("ns",):
        return int(val)
    if unit_str in ("us", "µs"):
        return int(val * MICROSECOND)
    if unit_str in ("ms",):
        return int(val * MILLISECOND)
    if unit_str in ("s",):
        return int(val * SECOND)
    if unit_str in ("m",):
        return int(val * MINUTE)
    if unit_str in ("h",):
        return int(val * HOUR)

    raise ValueError(f"Invalid duration unit: {unit_str!r}")


# ---- Cancellation context ----
class ContextWithCancel:
    def __init__(self) -> None:
        self._cancelled = False

    def cancel(self) -> None:
        self._cancelled = True

    # Provide attributes to mimic the Rust struct presence when passed around
    def __repr__(self) -> str:
        return f"ContextWithCancel(cancelled={self._cancelled})"


# ---- Options structs ----
class ClientOptions:
    def __init__(self) -> None:
        # These attributes mirror what the higher-level Python code sets
        self.logger: Optional[PythonLogger] = None
        self.ws_url: Optional[str] = None
        self.reverse: Optional[bool] = None
        self.socks_host: Optional[str] = None
        self.socks_port: Optional[int] = None
        self.socks_username: Optional[str] = None
        self.socks_password: Optional[str] = None
        self.socks_wait_server: Optional[bool] = None
        self.reconnect: Optional[bool] = None
        self.reconnect_delay: Optional[int] = None  # ns
        self.buffer_size: Optional[int] = None
        self.channel_timeout: Optional[int] = None  # ns
        self.connect_timeout: Optional[int] = None  # ns
        self.threads: Optional[int] = None
        self.fast_open: Optional[bool] = None
        self.upstream_proxy: Optional[str] = None
        self.upstream_username: Optional[str] = None
        self.upstream_password: Optional[str] = None
        self.no_env_proxy: Optional[bool] = None
        self.user_agent: Optional[str] = None


class ServerOptions:
    def __init__(self) -> None:
        self.logger: Optional[PythonLogger] = None
        self.ws_host: Optional[str] = None
        self.ws_port: Optional[int] = None
        self.socks_host: Optional[str] = None
        self.port_pool: Optional[Any] = None
        self.socks_wait_client: Optional[bool] = None
        self.buffer_size: Optional[int] = None
        self.api_key: Optional[str] = None
        self.channel_timeout: Optional[int] = None  # ns
        self.connect_timeout: Optional[int] = None  # ns
        self.fast_open: Optional[bool] = None
        self.upstream_proxy: Optional[str] = None
        self.upstream_username: Optional[str] = None
        self.upstream_password: Optional[str] = None


# ---- Return structs resembling Rust binding objects ----
@dataclass
class ReverseTokenResult:
    token: str
    port: int


# ---- Client/Server minimal shims ----
class Client:
    def __init__(self, token: str, options: ClientOptions) -> None:
        self._token = token
        self._options = options
        self._closed = False
        # Expose a few fields that higher-level code might introspect
        self.socks_port: Optional[int] = getattr(options, "socks_port", None)
        self.is_connected: bool = True  # pretend connected for tests

        # Emit a small info log so the log pipeline can be exercised
        if options.logger:
            options.logger.info("client-initialized")

    def wait_ready(self, ctx: Optional[ContextWithCancel], timeout_ns: int) -> None:
        # No-op: simulate immediate readiness
        return None

    def add_connector(self, connector_token: Optional[str]) -> str:
        return connector_token or str(uuid.uuid4())

    def close(self) -> None:
        if not self._closed:
            self._closed = True
            if self._options.logger:
                self._options.logger.info("client-closed")


class Server:
    def __init__(self, options: ServerOptions) -> None:
        self._options = options
        self._closed = False
        if options.logger:
            options.logger.info("server-initialized")

    def wait_ready(self, ctx: Optional[ContextWithCancel], timeout_ns: int) -> None:
        # No-op
        return None

    def add_forward_token(self, token: Optional[str]) -> str:
        return token or str(uuid.uuid4())

    def add_reverse_token(self, opts: Any) -> ReverseTokenResult:
        token = getattr(opts, "token", None) or str(uuid.uuid4())
        port = int(getattr(opts, "port", 0) or 0) or 1080
        return ReverseTokenResult(token=token, port=port)

    def add_connector_token(self, connector_token: Optional[str], reverse_token: str) -> str:
        return connector_token or str(uuid.uuid4())

    def remove_token(self, token: str) -> bool:
        # Always succeed for shim
        return True

    def close(self) -> None:
        if not self._closed:
            self._closed = True
            if self._options.logger:
                self._options.logger.info("server-closed")