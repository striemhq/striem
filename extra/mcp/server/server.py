import importlib.util
import os
from pathlib import Path
import pkgutil
import sys

from fastmcp import FastMCP
from fastmcp.server.providers import LocalProvider

tools = LocalProvider()

def wrapper(fn):
    def wrapped(event: dict):
        print(f"Calling {fn.__name__} with event: {event}")
        return fn(event)
    return wrapped

def load_actions():
    actions_path = Path(os.getenv("STRIEM_ACTIONS_PATH", "actions"))

    try:
        actions_path.mkdir(exist_ok=True, parents=True)
    except Exception as e:
        print(f"Error creating actions directory: {e}")

    _s = importlib.util.spec_from_loader('actions', loader=None, is_package=True)
    if _s is None:
        raise ImportError("Could not create module spec for 'actions'")

    _s.submodule_search_locations = [str(actions_path)]
    _a = importlib.util.module_from_spec(_s)
    _a.__path__ = pkgutil.extend_path(_a.__path__, 'actions')

    sys.modules['actions'] = _a

    for _, _m, _ in pkgutil.iter_modules(path=[str(actions_path)]):
        mod = importlib.import_module(f'actions.{_m}')
        for sym in dir(mod):
            if sym.startswith('_'):
                continue
            fn = getattr(mod, sym)
            if callable(fn):
                if fn.__doc__:
                    print(f"Registering action: {sym} - {fn.__doc__.strip()}")
                else:
                    print(f"Registering action: {sym} - No docstring provided")
                wrapped = wrapper(fn)
                wrapped.__name__ = f'{sym}.{fn.__name__}'
                wrapped.__doc__ = fn.__doc__
                tools.tool()(wrapped)

if __name__ == "__main__":
    load_actions()
    mcp = FastMCP("StrIEM Actions", providers=[tools])
    mcp.run(transport="http", host=os.getenv("STRIEM_MCP_HOST", "0.0.0.0"), port=int(os.getenv("STRIEM_MCP_PORT", 6000)))
