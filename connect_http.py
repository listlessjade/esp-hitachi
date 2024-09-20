import requests
import json
import time

script = open("test.rhai").read()

r = requests.post("http://192.168.0.126:8080/post", data=json.dumps({
    "method": "mgmt:set_script",
    "id": 0,
    "params": script
})).json()
print(r)

# time.sleep(3)

r = requests.post("http://192.168.0.126:8080/post", data=json.dumps({
    "method": "mgmt:restart",
    "id": 0,
    "params": script
})).json()
print(r)
time.sleep(5)


# r = requests.post("http://192.168.0.126:8080/post", data=json.dumps({
#     "method": "rpc:set_percent",
#     "id": 1,
#     "params": [50]
# })).json()
# print(r)