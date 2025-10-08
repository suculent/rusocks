#!/usr/bin/env python3
"""
Setup script for rusocks Python bindings.

Rusocks is a SOCKS proxy implementation over WebSocket protocol.
This package provides Python bindings for the Rust implementation.
"""

import os
import sys
import shutil
import subprocess
import platform
import tempfile
import importlib.machinery
from pathlib import Path
from setuptools import setup, find_packages
import setuptools
from urllib.request import urlretrieve
from setuptools.command.sdist import sdist as _sdist
from setuptools.command.build_py import build_py as _build_py
from setuptools.command.develop import develop as _develop
from setuptools.command.install import install as _install
from typing import Optional

# Get the current directory
here = Path(__file__).parent.absolute()

# Global variables
_temp_rust_dir = None

# Platform-specific configurations
install_requires = [
    "setuptools>=40.0",
    "click>=8.0",
    "loguru",
    "rich",
]

# Development dependencies
extras_require = {
    "dev": [
        "pytest>=6.0",
        "pytest-cov>=2.10",
        "pytest-mock>=3.0",
        "pytest-xdist",
        "black>=21.0",
        "flake8>=3.8",
        "mypy>=0.800",
        "httpx[socks]",
        "requests",
        "pysocks",
    ],
}

def ensure_placeholder_rusockslib():
    """Ensure a placeholder Python package exists so find_packages() includes it.
    The actual native bindings will be generated later during the build step.
    """
    pkg_dir = here / "rusockslib"
    init_py = pkg_dir / "__init__.py"
    try:
        if not pkg_dir.exists():
            pkg_dir.mkdir(parents=True, exist_ok=True)
        if not init_py.exists():
            init_py.write_text("# Placeholder; real contents generated during build\n")
    except Exception as e:
        print(f"Warning: failed to create placeholder rusockslib: {e}")

def prepare_rust_sources():
    """Prepare Rust source files by copying them to a temporary directory."""
    rust_src_dir = here / "rust_src"
    
    # If rust_src already exists (e.g., from source distribution), use it
    if rust_src_dir.exists():
        print(f"Using existing Rust sources in {rust_src_dir}")
        return rust_src_dir
    
    print("Preparing Rust source files...")
    
    # Try to find project root (go up from _bindings/python/)
    project_root = here.parent.parent
    if not (project_root / "Cargo.toml").exists():
        raise FileNotFoundError("Cannot find project root with Cargo.toml file")
    
    # Create rust_src directory
    if rust_src_dir.exists():
        shutil.rmtree(rust_src_dir)
    rust_src_dir.mkdir()
    
    # Copy Cargo.toml to parent directory (here)
    for file in ["Cargo.toml", "Cargo.lock"]:
        src = project_root / file
        if src.exists():
            shutil.copy2(src, here / file)
            print(f"Copied {file} to {here}")
    
    # Copy rusocks Rust files to rust_src directory
    rusocks_src = project_root / "src"
    if rusocks_src.exists():
        for rust_file in rusocks_src.glob("*.rs"):
            shutil.copy2(rust_file, rust_src_dir / rust_file.name)
            print(f"Copied {rust_file.name} to rust_src/")
    else:
        raise FileNotFoundError("Cannot find rusocks source directory")
    
    print(f"Rust sources prepared in {rust_src_dir}")
    return rust_src_dir

def _expected_binary_names() -> list[str]:
    """Return candidate filenames for the extension for the current interpreter/platform."""
    candidates: list[str] = []
    for suffix in importlib.machinery.EXTENSION_SUFFIXES:
        # e.g. ['.cpython-311-x86_64-linux-gnu.so', '.so']
        candidates.append(f"_rusockslib{suffix}")
    # Also include conservative fallbacks by version tag just in case
    pyver = f"{sys.version_info.major}{sys.version_info.minor}"
    candidates.append(f"_rusockslib.cpython-{pyver}.so")
    candidates.append(f"_rusockslib.cp{pyver}.pyd")
    return candidates

def is_rusockslib_built(lib_dir: Path) -> bool:
    """Determine if rusockslib contains a native artifact compatible with this Python."""
    if not lib_dir.exists():
        return False
    for name in _expected_binary_names():
        if (lib_dir / name).exists():
            return True
    return False

def prune_foreign_binaries(lib_dir: Path) -> None:
    """Remove artifacts that are not compatible with the current interpreter.

    This prevents wheels for one Python version from accidentally bundling
    binaries produced for a different version/ABI.
    """
    if not lib_dir.exists():
        return
    keep_names = set(_expected_binary_names())
    for p in lib_dir.iterdir():
        if not p.is_file():
            continue
        if p.name.startswith("_rusockslib") and p.suffix in {".so", ".pyd", ".dll", ".dylib"}:
            if p.name not in keep_names:
                try:
                    p.unlink()
                    print(f"Pruned foreign binary: {p}")
                except Exception as e:
                    print(f"Warning: failed to remove {p}: {e}")

def run_command(cmd, cwd=None, env=None):
    """Run a command and return the result."""
    print(f"Running: {' '.join(cmd)}")
    try:
        # Use current environment if no env is provided
        if env is None:
            env = os.environ.copy()
        result = subprocess.run(
            cmd, 
            cwd=cwd, 
            env=env, 
            capture_output=True, 
            text=True, 
            check=True
        )
        return result.stdout.strip()
    except subprocess.CalledProcessError as e:
        print(f"Command failed: {e}")
        print(f"stdout: {e.stdout}")
        print(f"stderr: {e.stderr}")
        raise

def check_rust_installation():
    """Check if Rust is installed and return version."""
    try:
        result = run_command(["rustc", "--version"])
        print(f"Found Rust: {result}")
        return True
    except (subprocess.CalledProcessError, FileNotFoundError):
        return False

def download_file(url, destination):
    """Download a file from URL to destination."""
    print(f"Downloading {url} to {destination}")
    urlretrieve(url, destination)

def install_rust():
    """Download and install Rust if not available."""
    global _temp_rust_dir
    
    if check_rust_installation():
        return
    
    print("Rust not found, downloading and installing to temporary directory...")
    
    # Determine platform and architecture
    system = platform.system().lower()
    
    # Create temporary directory for Rust installation (don't delete it yet)
    _temp_rust_dir = tempfile.mkdtemp(prefix="rust_install_")
    temp_dir_path = Path(_temp_rust_dir)
    
    try:
        # Download rustup-init
        if system == "windows":
            rustup_file = temp_dir_path / "rustup-init.exe"
            rustup_url = "https://win.rustup.rs/x86_64"
        else:  # Unix-like
            rustup_file = temp_dir_path / "rustup-init.sh"
            rustup_url = "https://sh.rustup.rs"
        
        download_file(rustup_url, rustup_file)
        
        # Make rustup-init executable on Unix-like systems
        if system != "windows":
            os.chmod(rustup_file, 0o755)
        
        # Install Rust
        print(f"Installing Rust to temporary directory: {temp_dir_path}")
        
        # Run rustup-init with -y to accept defaults
        if system == "windows":
            run_command([str(rustup_file), "-y"], cwd=temp_dir_path)
        else:
            run_command([str(rustup_file), "-y", "--no-modify-path"], cwd=temp_dir_path)
        
        # Update PATH to include Rust binaries
        if system == "windows":
            cargo_bin = Path.home() / ".cargo" / "bin"
        else:
            cargo_bin = Path.home() / ".cargo" / "bin"
        
        # Update PATH
        current_path = os.environ.get("PATH", "")
        if str(cargo_bin) not in current_path:
            os.environ["PATH"] = f"{cargo_bin}{os.pathsep}{current_path}"
        
        print(f"Updated PATH to include Rust: {cargo_bin}")
        
        print("Rust installed successfully")
        
    except Exception as e:
        # Clean up on error
        if _temp_rust_dir and Path(_temp_rust_dir).exists():
            shutil.rmtree(_temp_rust_dir)
            _temp_rust_dir = None
        raise e

def cleanup_temp_rust():
    """Clean up temporary Rust installation."""
    global _temp_rust_dir
    if _temp_rust_dir and Path(_temp_rust_dir).exists():
        print(f"Cleaning up temporary Rust installation: {_temp_rust_dir}")
        try:
            # Try to make files writable before deletion
            import stat
            for root, dirs, files in os.walk(_temp_rust_dir):
                for d in dirs:
                    os.chmod(os.path.join(root, d), stat.S_IRWXU | stat.S_IRWXG | stat.S_IRWXO)
                for f in files:
                    os.chmod(os.path.join(root, f), stat.S_IRWXU | stat.S_IRWXG | stat.S_IRWXO)
            shutil.rmtree(_temp_rust_dir)
            _temp_rust_dir = None
        except Exception as e:
            print(f"Warning: Failed to clean up temporary Rust installation: {e}")
            _temp_rust_dir = None

def install_pyo3_and_tools():
    """Install PyO3 and related Rust tools."""
    print("Installing PyO3 and Rust tools...")
    
    # Ensure Rust is available
    if not check_rust_installation():
        raise RuntimeError("Rust is not available after installation attempt")
    
    # Install maturin for building Python bindings
    try:
        run_command(["pip", "install", "maturin"])
        print("maturin installed successfully")
    except subprocess.CalledProcessError as e:
        print(f"Failed to install maturin: {e}")
        raise

def build_python_bindings():
    """Build Python bindings using maturin."""
    print("Building Python bindings with maturin...")
    
    # Prepare Rust sources first
    rust_src_dir = prepare_rust_sources()
    
    try:
        # Clean existing bindings
        rusocks_lib_dir = here / "rusockslib"
        if rusocks_lib_dir.exists():
            shutil.rmtree(rusocks_lib_dir)
            print(f"Cleaned existing {rusocks_lib_dir}")
        
        # Set up environment
        env = os.environ.copy()
        env["RUSTFLAGS"] = "-C target-feature=+crt-static"
        
        # Run maturin build
        cmd = [
            "maturin", "build",
            "--release",
            "--strip",
            "--out", str(rusocks_lib_dir),
        ]
        
        run_command(cmd, cwd=here, env=env)
        
        print("Python bindings built successfully")
        # After a successful build, prune any binaries not matching current ABI
        prune_foreign_binaries(rusocks_lib_dir)
        
    finally:
        # Clean up temporary rust_src directory and Cargo.toml/Cargo.lock
        if rust_src_dir.exists():
            shutil.rmtree(rust_src_dir)
            print(f"Cleaned up {rust_src_dir}")
        
        for file in ["Cargo.toml", "Cargo.lock"]:
            temp_rust_file = here / file
            if temp_rust_file.exists():
                temp_rust_file.unlink()
                print(f"Cleaned up {temp_rust_file}")

def ensure_python_bindings():
    """Ensure Python bindings are available, build if necessary."""
    rusocks_lib_dir = here / "rusockslib"
    local_rust_src_dir = here / "rust_src"
    local_cargo_toml = here / "Cargo.toml"
    
    # Decide based on whether a binding for THIS interpreter exists
    if not is_rusockslib_built(rusocks_lib_dir):
        print("rusockslib not built or only placeholder found, building Python bindings...")
        
        # Determine availability of Rust sources
        have_local_sources = local_rust_src_dir.exists() and local_cargo_toml.exists()
        if not have_local_sources:
            # Fallback to project root layout (building from repo)
            try:
                project_root = here.parent.parent
                if not (project_root / "Cargo.toml").exists():
                    raise FileNotFoundError("Cannot find project root with Cargo.toml file")
            except Exception:
                raise RuntimeError(
                    "Cannot find Rust source files. "
                    "This package should be built from the rusocks source repository, "
                    "or you should use a pre-built wheel."
                )
        
        # Check if we have Rust available
        if not check_rust_installation():
            print("Rust not found, attempting to install...")
            try:
                install_rust()
            except Exception as e:
                print(f"Failed to install Rust: {e}")
                raise RuntimeError(
                    "Rust is required to build rusocks from source. "
                    "Please install Rust from https://rustup.rs/ or use a pre-built wheel."
                )
        
        try:
            # Install PyO3 and tools
            install_pyo3_and_tools()
            
            # Build bindings
            build_python_bindings()
            
        except Exception as e:
            print(f"Failed to build Python bindings: {e}")
            raise RuntimeError(
                f"Failed to build rusocks from source: {e}\n"
                "This may be due to missing dependencies or incompatible system.\n"
                "Try installing a pre-built wheel or ensure Rust is installed."
            )
        finally:
            # Clean up temporary Rust installation
            cleanup_temp_rust()
        
        if not is_rusockslib_built(rusocks_lib_dir):
            raise RuntimeError("Failed to build Python bindings (artifacts missing)")
    else:
        # Ensure we only ship binaries compatible with this interpreter
        prune_foreign_binaries(rusocks_lib_dir)
        print(f"Found existing built rusockslib at {rusocks_lib_dir}")

def test_bindings():
    """Test if the Python bindings work correctly."""
    try:
        # Try to import the bindings
        sys.path.insert(0, str(here))
        import rusockslib
        print("✓ Python bindings imported successfully")
        
        # Try to access some basic functionality
        if hasattr(rusockslib, '__version__') or hasattr(rusockslib, 'NewClient'):
            print("✓ Python bindings appear to be functional")
        else:
            print("⚠ Python bindings imported but may not be fully functional")
        
        return True
    except ImportError as e:
        print(f"✗ Failed to import Python bindings: {e}")
        return False
    except Exception as e:
        print(f"✗ Error testing Python bindings: {e}")
        return False
    finally:
        # Clean up sys.path
        if str(here) in sys.path:
            sys.path.remove(str(here))

# Read description from README
def get_long_description():
    """Get long description from README file."""
    # Use local README
    local_readme = here / "README.md"
    if local_readme.exists():
        with open(local_readme, "r", encoding="utf-8") as f:
            return f.read()
    else:
        # Fallback to a simple description
        return "Python bindings for Rusocks - a SOCKS proxy implementation over WebSocket protocol."

class SdistWithRustSources(_sdist):
    """Custom sdist that ensures Rust sources exist
    before creating the source distribution, and cleans them afterwards.
    """

    def run(self):
        rust_src_dir = None
        created_files = []
        try:
            rust_src_dir = prepare_rust_sources()
            # Track Cargo.toml and Cargo.lock created in this directory for cleanup
            for fname in ["Cargo.toml", "Cargo.lock"]:
                fpath = here / fname
                if fpath.exists():
                    created_files.append(fpath)
            super().run()
        finally:
            # Clean up generated Rust sources and module files after sdist
            try:
                if rust_src_dir and Path(rust_src_dir).exists():
                    shutil.rmtree(rust_src_dir)
                    print(f"Cleaned up {rust_src_dir}")
            except Exception as cleanup_err:
                print(f"Warning: failed to remove {rust_src_dir}: {cleanup_err}")
            for fpath in created_files:
                try:
                    if fpath.exists():
                        fpath.unlink()
                        print(f"Cleaned up {fpath}")
                except Exception as cleanup_err:
                    print(f"Warning: failed to remove {fpath}: {cleanup_err}")


class BuildPyEnsureBindings(_build_py):
    """Ensure Python bindings exist when building the package (wheel/install).

    This avoids heavy work at import time and only triggers during actual builds.
    """

    def run(self):
        # Ensure placeholder so that wheel metadata captures the package
        ensure_placeholder_rusockslib()
        try:
            ensure_python_bindings()
        except Exception as e:
            # Do not fail metadata-only operations; re-raise for real builds
            if os.environ.get("SETUPTOOLS_BUILD_META", ""):  # PEP 517 builds
                raise
            raise
        # Just in case, prune leftovers again before packaging
        prune_foreign_binaries(here / "rusockslib")
        super().run()


class DevelopEnsureBindings(_develop):
    """Ensure bindings exist for editable installs (pip install -e .)."""

    def run(self):
        ensure_placeholder_rusockslib()
        ensure_python_bindings()
        super().run()


class InstallEnsureBindings(_install):
    """Ensure bindings exist for regular installs (pip install .)."""

    def run(self):
        ensure_placeholder_rusockslib()
        ensure_python_bindings()
        super().run()

class BinaryDistribution(setuptools.Distribution):
    def has_ext_modules(_):
        return True

# Ensure placeholder package exists BEFORE calling setup() so find_packages() sees it
ensure_placeholder_rusockslib()

setup(
    name="rusocks",
    version="1.0.0",
    description="Python bindings for Rusocks - SOCKS proxy over WebSocket",
    long_description=get_long_description(),
    long_description_content_type="text/markdown",
    author="igraczech",
    url="https://github.com/igraczech/rusocks",
    license="MIT",
    
    # Package configuration
    packages=find_packages(include=["rusockslib", "rusockslib.*", "rusocks"]),
    package_data={
        # Include native artifacts and helper sources generated by maturin
        "rusockslib": ["*.py", "*.so", "*.pyd", "*.dll", "*.dylib", "*.h", "*.c", "*.rs"],
    },
    include_package_data=True,
    
    # Dependencies
    install_requires=install_requires,
    extras_require=extras_require,
    
    # Python version requirement
    python_requires=">=3.9",
    
    # Metadata
    classifiers=[
        "Development Status :: 4 - Beta",
        "Intended Audience :: Developers",
        "Intended Audience :: System Administrators",
        "Operating System :: POSIX :: Linux",
        "Operating System :: MacOS",
        "Operating System :: Microsoft :: Windows",
        "Programming Language :: Python :: 3",
        "Programming Language :: Python :: 3.9",
        "Programming Language :: Python :: 3.10",
        "Programming Language :: Python :: 3.11",
        "Programming Language :: Python :: 3.12",
        "Programming Language :: Rust",
        "Topic :: Internet :: Proxy Servers",
        "Topic :: Internet :: WWW/HTTP",
        "Topic :: Software Development :: Libraries :: Python Modules",
        "Topic :: System :: Networking",
    ],
    keywords="socks proxy websocket network tunneling firewall bypass load-balancing rust bindings",
    
    # Entry points
    entry_points={
        "console_scripts": [
            "rusocks=rusocks._cli:cli",
        ],
    },
    
    # Build configuration
    zip_safe=False,  # Due to binary extensions
    platforms=["any"],
    
    # Project URLs
    project_urls={
        "Bug Reports": "https://github.com/igraczech/rusocks/issues",
        "Source": "https://github.com/igraczech/rusocks",
        "Documentation": "https://github.com/igraczech/rusocks#readme",
    },
    
    # Binary distribution
    distclass=BinaryDistribution,
    cmdclass={
        "sdist": SdistWithRustSources,
        "build_py": BuildPyEnsureBindings,
        "develop": DevelopEnsureBindings,
        "install": InstallEnsureBindings,
    },
)