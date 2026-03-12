let socket: WebSocket;

type Listener<T> = (value: T) => void
class State<T> { 
    private listeners: Listener<T>[] = []; 

    constructor(private value: T) {}
     
    get(): T { 
        return this.value;
    } 

    set(newValue: T): void { 
        if (this.value !== newValue) { 
            this.value = newValue;
            this.notify();
        } 
    }

    subscribe(listener: Listener<T>): () => void {
        this.listeners.push(listener); 
        return () => {
            this.listeners = this.listeners.filter(l => l !== listener);
        }; 
    }

    private notify(): void { 
        this.listeners.forEach(listener => listener(this.value)); 
    } 
}

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

    const ticker_state: State<string> = new State<string>("");
    const ticker_input = document.getElementById("ticker_input") as HTMLInputElement;
    ticker_input.addEventListener("input", () => {
        const ticker: string = ticker_input.value.toUpperCase();
        ticker_input.value = ticker;
        ticker_state.set(ticker);
    });

    const robotrader_tabs: NodeListOf<Element> = document.querySelectorAll(".robotrader_tabs");
    for (let i = 0; i < robotrader_tabs.length; i++) {
        const robotrader_tab: HTMLElement = robotrader_tabs[i] as HTMLElement;
        robotrader_tab.addEventListener("click", () => {
            for (let j = 0; j < robotrader_tabs.length; j++) // remove active from all tabs
                robotrader_tabs[j].classList.remove("active");
            
            robotrader_tab.classList.add("active"); // activate clicked tab
            updateActiveTabInfo();
        });
    }

    function updateActiveTabInfo() {
        const activeTab: HTMLElement = document.querySelector(".robotrader_tabs.active") as HTMLElement;
        if (activeTab == null)
            return;

        const ticker: string = ticker_state.get();
        const robotrader_info_div: HTMLElement = document.getElementById("robotrader_tabs_info") as HTMLElement;
        robotrader_info_div.innerText = `Ticker: ${ticker} | Tab: ${activeTab.innerText}`;
    }
    
    ticker_state.subscribe(() => { updateActiveTabInfo(); }); // keep input synced with state
};