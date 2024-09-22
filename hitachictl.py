import requests
import json
import time
import click
import subprocess
from click_repl import register_repl

def call_rpc(method, parameters, timeout=10):
    return requests.post("http://192.168.0.126:8080/post", data=json.dumps({
        "method": method,
        "id": 0,
        "params": parameters
    }), timeout=timeout).json()

@click.group()
def cli():
    pass

@cli.command()
@click.argument('script', type=click.File('r'))
def set_script(script):
    data = script.read()
    r = requests.post('http://192.168.0.126:8080/ota/script', data=data, headers={'Content-Type': 'application/octet-stream'})
    print(f"Uploaded script: {r.text}")
    print("Requesting recompile...")
    print(call_rpc("mgmt:recompile", []))

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
    print(f"Response:", call_rpc(f"rpc:{method}", list(args)))

@cli.command()
@click.argument('new_image', type=click.File('rb'))
def update_firmware(new_image):
    firmware = new_image.read()
    r = requests.post('http://192.168.0.126:8080/ota/firmware', data=firmware, headers={'Content-Type': 'application/octet-stream'})
    print(r.text)

@cli.command()
def build_update_firmware():
    subprocess.run(["cargo", "build", "--release"], check=True)
    subprocess.run(["espflash", "save-image", "--chip", "esp32c6", "-s", "8mb", "target/riscv32imac-esp-espidf/release/esp-hitachi", "/tmp/hitachi.bin"])
    firmware = open('/tmp/hitachi.bin', 'rb').read()
    r = requests.post('http://192.168.0.126:8080/ota/firmware', data=firmware, headers={'Content-Type': 'application/octet-stream'})
    print(r.text)

register_repl(cli)
cli()
# time.sleep(3)

# r = requests.post("http://192.168.0.126:8080/post", data=json.dumps({
#     "method": "mgmt:restart",
#     "id": 0,
#     "params": script
# })).json()
# print(r)
# time.sleep(5)


# r = requests.post("http://192.168.0.126:8080/post", data=json.dumps({
#     "method": "rpc:set_percent",
#     "id": 1,
#     "params": [50]
# })).json()
# print(r)