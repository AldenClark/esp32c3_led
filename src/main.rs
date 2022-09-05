use embedded_svc::mqtt::client::MessageImpl;
use embedded_svc::mqtt::client::{Client, Connection, Event, Message, QoS};
use embedded_svc::wifi::{self, Wifi};
use esp_idf_svc::nvs::EspDefaultNvs;
use esp_idf_svc::wifi::EspWifi;
use esp_idf_svc::{mqtt, netif, sysloop};
use esp_idf_sys::EspError;
use serde_json::{from_str, Map, Value};
use smart_leds_trait::SmartLedsWrite;
use std::str::FromStr;
use std::{
    sync::{
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
use ws2812_esp32_rmt_driver::Ws2812Esp32Rmt;

const WIFI_SSID: &str = "************";
const WIFI_PASSWORD: &str = "**********************";

const MQTT_URL: &str = "mqtt://broker-cn.emqx.io;1883";
const MQTT_CLIENTID: &str = "esp32_c3";
const MQTT_SUB_TOPIC: &str = "esp32_c3_sub";
const MQTT_QOS: QoS = QoS::AtMostOnce;

fn main() -> Result<(), EspError> {
    //WiFi
    println!("开始连接WiFi");
    let mut wifi = EspWifi::new(
        Arc::new(netif::EspNetifStack::new()?),
        Arc::new(sysloop::EspSysLoopStack::new()?),
        Arc::new(EspDefaultNvs::new()?),
    )?;
    loop {
        match wifi.set_configuration(&wifi::Configuration::Client(wifi::ClientConfiguration {
            ssid: WIFI_SSID.into(),
            password: WIFI_PASSWORD.into(),
            ..Default::default()
        })) {
            Ok(_) => break,
            Err(error) => println!("WiFi连接失败 : {}", error),
        }
    }
    loop {
        println!("正在获取WiFi连接状态");
        match wifi.get_status() {
            wifi::Status(
                wifi::ClientStatus::Started(wifi::ClientConnectionStatus::Connected(
                    wifi::ClientIpStatus::Done(_),
                )),
                _,
            ) => {
                println!("WiFi连接成功");
                std::thread::sleep(std::time::Duration::from_secs(2));
                break;
            }
            _ => continue,
        }
    }

    // ARC
    let led_loop = Arc::new(Mutex::new(0));

    // MQTT
    let mqtt_config = mqtt::client::MqttClientConfiguration {
        client_id: Some(MQTT_CLIENTID),
        ..Default::default()
    };
    let (mut mqtt_client, mut mqtt_conn) =
        match mqtt::client::EspMqttClient::new_with_conn(MQTT_URL, &mqtt_config) {
            Ok((mqtt_client, mqtt_conn)) => (mqtt_client, mqtt_conn),
            Err(_) => todo!(),
        };
    println!("MQTT已连接");

    //mqtt thread
    let (led_tx, led_rx): (SyncSender<MessageImpl>, Receiver<MessageImpl>) = mpsc::sync_channel(0);
    let led_loop_tx = Arc::clone(&led_loop);
    let mqtt_thread = thread::spawn(move || {
        while let Some(msg) = mqtt_conn.next() {
            match msg {
                Ok(msg) => {
                    if let Event::Received(message) = msg {
                        {
                            let mut led_loop_status = led_loop_tx.lock().unwrap();
                            if *led_loop_status == 1 {
                                *led_loop_status = 0
                            } else {
                                *led_loop_status = 1
                            }
                        }
                        match led_tx.send(message) {
                            Ok(_) => println!("收到MQTT消息并传递成功。"),
                            Err(e) => println!("收到MQTT消息但传递失败：{e}"),
                        };
                    }
                }
                Err(e) => println!("MQTT错误：{e}"),
            }
        }
    });

    //mqtt subscribe
    match Client::subscribe(&mut mqtt_client, MQTT_SUB_TOPIC, MQTT_QOS) {
        Ok(_) => println!("MQTT已订阅成功。"),
        Err(error) => println!("MQTT订阅失败：{}", error),
    };

    //LED RMT
    let led_loop_rx = Arc::clone(&led_loop);
    let led = thread::spawn(move || loop {
        let data: Map<String, Value> =
            serde_json::from_str(std::str::from_utf8(led_rx.recv().unwrap().data()).unwrap())
                .unwrap();
        let gpio: u32 = u32::from_str(data.get("gpio").unwrap().to_string().as_str()).unwrap();
        let channel: u8 = from_str(data.get("channel").unwrap().to_string().as_str()).unwrap();
        let mut ws2812 = Ws2812Esp32Rmt::new(channel, gpio).unwrap();
        match data.get("led_power").unwrap().to_string().as_str() {
            "0" => {
                let pice: usize = from_str(data.get("pice").unwrap().to_string().as_str()).unwrap();
                match ws2812.write(std::iter::repeat((0, 0, 0)).take(pice)) {
                    Ok(_) => println!("LED灯带已关闭。"),
                    Err(e) => println!("LED灯带关闭失败：{e}"),
                };
            }
            "1" => match data.get("action").unwrap().to_string().as_str() {
                "1" => {
                    let r: u8 = from_str(data.get("r").unwrap().to_string().as_str()).unwrap();
                    let g: u8 = from_str(data.get("g").unwrap().to_string().as_str()).unwrap();
                    let b: u8 = from_str(data.get("b").unwrap().to_string().as_str()).unwrap();
                    let pice: usize =
                        from_str(data.get("pice").unwrap().to_string().as_str()).unwrap();
                    match ws2812.write(std::iter::repeat((g, r, b)).take(pice)) {
                        Ok(_) => {
                            println!("LED灯带已经设置为单色模式。");
                            let mut led_loop_status = led_loop_rx.lock().unwrap();
                            *led_loop_status = 0
                        }
                        Err(e) => println!("LED灯带设置为单色模式失败 : {e}"),
                    }
                }
                "2" => {
                    let pice: usize =
                        from_str(data.get("pice").unwrap().to_string().as_str()).unwrap();
                    let sleep_millis: u64 =
                        from_str(data.get("sleep_millis").unwrap().to_string().as_str()).unwrap();
                    loop {
                        std::thread::sleep(Duration::from_millis(sleep_millis));
                        {
                            let mut led_loop_status = led_loop_rx.lock().unwrap();
                            if *led_loop_status == 0 {
                                break *led_loop_status = 1;
                            }
                        }
                        let r: u8 = unsafe { esp_idf_sys::esp_random() } as u8;
                        let g: u8 = unsafe { esp_idf_sys::esp_random() } as u8;
                        let b: u8 = unsafe { esp_idf_sys::esp_random() } as u8;
                        ws2812
                            .write(std::iter::repeat((r, g, b)).take(pice))
                            .unwrap()
                    }
                }
                "3" => {
                    let pice: usize =
                        from_str(data.get("pice").unwrap().to_string().as_str()).unwrap();
                    let sleep_millis: u64 =
                        from_str(data.get("sleep_millis").unwrap().to_string().as_str()).unwrap();
                    loop {
                        {
                            let mut led_loop_status = led_loop_rx.lock().unwrap();
                            if *led_loop_status == 0 {
                                break *led_loop_status = 1;
                            }
                        }
                        for i in [
                            4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56, 60, 64, 68, 72,
                            76, 80, 84, 88, 92, 96, 100, 108, 116, 124, 132, 140, 148, 156, 164,
                            172, 180, 188, 196, 204, 220, 236, 220, 204, 196, 188, 180, 172, 164,
                            156, 148, 140, 132, 124, 116, 108, 100, 96, 92, 88, 84, 80, 76, 72, 68,
                            64, 60, 56, 52, 48, 44, 40, 36, 32, 28, 24, 20, 16, 12, 8, 4,
                        ]
                        .into_iter()
                        {
                            ws2812
                                .write(std::iter::repeat((i, i, i)).take(pice))
                                .unwrap();
                            std::thread::sleep(Duration::from_millis(sleep_millis * 2));
                            ws2812
                                .write(std::iter::repeat((0, 0, 0)).take(pice))
                                .unwrap();
                            std::thread::sleep(Duration::from_millis(sleep_millis));
                        }
                    }
                }
                "4" => {
                    let pice: usize =
                        from_str(data.get("pice").unwrap().to_string().as_str()).unwrap();
                    let sleep_millis: u64 =
                        from_str(data.get("sleep_millis").unwrap().to_string().as_str()).unwrap();
                    loop {
                        {
                            let mut led_loop_status = led_loop_rx.lock().unwrap();
                            if *led_loop_status == 0 {
                                break *led_loop_status = 1;
                            }
                        }
                        for i in (1..255).into_iter() {
                            ws2812
                                .write(std::iter::repeat((i, i, i)).take(pice))
                                .unwrap();
                            std::thread::sleep(Duration::from_millis(sleep_millis));
                        }
                    }
                }
                "5" => loop {
                    let sleep_millis: u64 =
                        from_str(data.get("sleep_millis").unwrap().to_string().as_str()).unwrap();
                    let pice: usize =
                        from_str(data.get("pice").unwrap().to_string().as_str()).unwrap();
                    {
                        let mut led_loop_status = led_loop_rx.lock().unwrap();
                        if *led_loop_status == 0 {
                            break *led_loop_status = 1;
                        }
                    }
                    for i in 1..(pice + 1) {
                        let color_green = std::iter::repeat((5, 0, 0)).take(5);
                        let color_red = std::iter::repeat((0, 5, 0)).take(5);
                        let color_blue = std::iter::repeat((0, 0, 5)).take(5);

                        let color_yellow = std::iter::repeat((5, 5, 0)).take(5);
                        let color_water = std::iter::repeat((0, 5, 5)).take(5);
                        let color_purple = std::iter::repeat((5, 0, 5)).take(5);

                        let color = color_red
                            .chain(color_green)
                            .chain(color_blue)
                            .chain(color_yellow)
                            .chain(color_water)
                            .chain(color_purple);

                        ws2812.write(color.cycle().skip(i).take(pice)).unwrap();

                        std::thread::sleep(Duration::from_millis(sleep_millis));
                    }
                },
                _ => println!("action字段错误。"),
            },
            _ => println!("led_power字段错误。"),
        }
    });

    mqtt_thread.join().unwrap();
    led.join().unwrap();

    Ok(())
}
