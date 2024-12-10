from .transports import *

def wpa2_personal_conf(ssid: str, password: str):
    return {
        'ssid': ssid,
        'authentication': {
            'type': 'personal',
            'password': password
        }
    }

def wpa2_enterprise_conf(ssid: str, identity: str, username: str, password: str):
    return {
        'ssid': ssid,
        'authentication': {
            'type': 'enterprise',
            'identity': identity,
            'username': username,
            'password': password
        }
    }

class Client():
    def __init__(self):
        self.http = HTTPRpc()
        self.ble = BLERpc()
        self.http_available = False

    async def find(self, use_ble=True, use_mdns=True, hitachi_addr=None):
        if use_ble:
            await self.ble.discover()
        self.http_available = await self.http.discover(use_mdns=use_mdns, hitachi_addr=hitachi_addr)
        
    async def make_call[T](self,  namespace: str, method: str, *args) -> RPCResponse[T]:
        if self.http_available:
            try:
                return await self.http.rpc_call(namespace, method, *args)
            except Exception as e:
                logging.warning(f"Failed to make call {namespace}:{method} over HTTP: {e}.") 
        
        try:
            return await self.ble.rpc_call(namespace, method, *args)
        except Exception as e:
            logging.error(f"Failed to make call over BLE: {e}")
    
    async def sys_set_wifi(self, config):
        return await self.make_call("conn", "set_wifi", [config])
    
    async def sys_restart(self):
        return await self.make_call("sys", "restart", [])
    
    async def conn_addresses(self):
        return await self.make_call("conn", "addr", [])
    
    async def uart_get_last(self) -> RPCResponse[str]:
        return await self.make_call("uart", "get_last", [])
    
    async def uart_send(self, msg: str):
        return await self.make_call("uart", "send", [msg])
    
    async def wand_get_percent(self) -> RPCResponse[int]:
        return await self.make_call("wand", "get_percent", [])
    
    async def wand_set_percent(self, pct: int):
        return await self.make_call("wand", "set_percent", [pct])

    async def wand_set_lovense_mappings(self, low: int, high: int):
        return await self.make_call("wand", "update_lovense_mapping", [low, high])
   
    async def wand_set_light_mappings(self, bottom: int, mid_low: int, mid_high: int, top: int):
        return await self.make_call("wand", "set_light_mappings", [bottom, mid_low, mid_high, top])
   
    async def wand_set_button_increments(self, bottom: int, top: int):
        return await self.make_call("wand", "set_button_increments", [bottom, top])
   
    async def sys_get_addr(self):
        return await self.make_call("conn", "addr", [])

    async def sys_build_info(self):
        return await self.make_call("sys", "build_info", [])
    
    async def sys_health(self):
        return await self.make_call("sys", "health", [])