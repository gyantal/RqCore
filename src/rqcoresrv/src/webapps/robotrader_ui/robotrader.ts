let socket: WebSocket;

window.onload = () => {
    const wifi_icon = document.getElementById("wifi_icon") as HTMLElement;
    const user_email = document.getElementById("user_email") as HTMLElement;

    const scheme = location.protocol === "https:" ? "wss" : "ws";
    const port = location.port ? ":" + location.port : "";
    const connection_url = scheme + "://" + location.hostname + port + "/ws/robotrader_websocket";

    socket = new WebSocket(connection_url);
    socket.onopen = () => {
        console.log("WebSocket connected");
        wifi_icon.style.fill = "limegreen";
    };

    socket.onmessage = (event: MessageEvent) => {
        let data = JSON.parse(event.data);
        switch(data.type) {
            case "onconnected":
                user_email.innerText = data.user;
                break;
            case "executed_orders":
                console.log(data.symbol, data.price);
                break;
        }
    };

    socket.onclose = () => {
        console.log("WebSocket closed");
        wifi_icon.style.fill = "red";
    };
};