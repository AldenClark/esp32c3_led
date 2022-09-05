# 基于ESP32-C3的ws2812控制代码

ESP32-C3 with ws2812

连接WiFi，订阅MQTT，通过MQTT对LED灯带进行控制，具体控制使用的json说明见后文

内置单色灯光、暴闪、呼吸灯、渐变、跑马灯等灯光样式，这些样式仅供参考，效果不好勿见怪，可以根据需要设置更多样式

main.rs内需要设置WiFi账号密码和MQTT服务器信息

## MQTT 消息格式

```json
// channel为LED灯带控制通道
// gpio为led灯带接的引脚
// pice为灯珠数量
// led_power : 1=开灯 0=关灯 ， 为0时，不解析其他参数
// action : 1=单色灯光  2=暴闪  3=呼吸灯  4=渐变   5=跑马灯
// r/g/b 为RGB色值，ws2812中，颜色为GRB，渐变和跑马灯状态时不需要传此值
// sleep_millis为暴闪、呼吸灯、渐变、跑马灯状态时的灯光切换时间
{
    "channel":0,
    "gpio":9,
    "pice":8,
    "led_power":1,    
    "action":1,   
    "r":33,
    "g":44,
    "b":55,
    "channel":0,
    "gpio":9,
    "pice":8,
    "sleep_millis":45
}
```
