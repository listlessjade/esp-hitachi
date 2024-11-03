import requests
import json
import time
import click
import subprocess
import os
from pprint import pprint
from click_repl import register_repl
from tqdm import tqdm
from tqdm.utils import CallbackIOWrapper

base_addr = "http://192.168.0.131"

def try_conv_int(val):
    try:
        return int(val)
    except ValueError:
        return val

def call_rpc(method, parameters, timeout=10):
    return requests.post(f"{base_addr}:8080/post", data=json.dumps({
        "method": method,
        "id": 0,
        "params": [try_conv_int(p) for p in parameters]
    }), timeout=timeout).json()

@click.group()
def cli():
    pass

@cli.command()
@click.argument('ssid')
@click.argument('password')
def wifi(ssid, password):
    ret = call_rpc("mgmt:set_wifi", [ssid, password])
    print(f"Updated wifi config: {ret}")


@cli.command()
def restart():
    ret = call_rpc("mgmt:restart", [], timeout=1)
    print(f"Restarted!")

@cli.command()
@click.argument('args', nargs=-1)
def lovense(args):
    print(f"Lovense response:", call_rpc("rpc:lovense", [args]))

@cli.command()
@click.argument('method')
@click.argument('args', nargs=-1)
def rpc(method, args):
    print(json.dumps(call_rpc(f"{method}", list(args)), indent=4))

@cli.command()
@click.argument('new_image', type=click.File('rb'))
def update_firmware(new_image):
    firmware = new_image.read()
    r = requests.post(f'{base_addr}:8080/ota/firmware', data=firmware, headers={'Content-Type': 'application/octet-stream'})
    print(r.text)

@cli.command()
def build_update_firmware():
    subprocess.run(["cargo", "build", "--release"], check=True)
    subprocess.run(["espflash", "save-image", "--chip", "esp32c6", "-s", "8mb", "target/riscv32imac-esp-espidf/release/esp-hitachi", "/tmp/hitachi.bin"])
    
    file_size = os.stat('/tmp/hitachi.bin').st_size
    firmware = open('/tmp/hitachi.bin', 'rb')
    
    with tqdm(total = file_size, unit = "B", unit_scale=True, unit_divisor=1024) as t:
        wrapped_file = CallbackIOWrapper(t.update, firmware, "read")
        r = requests.post(f'{base_addr}:8080/ota/firmware', data=wrapped_file, headers={'Content-Type': 'application/octet-stream'})
    print(r.text)

    

register_repl(cli)
cli()
