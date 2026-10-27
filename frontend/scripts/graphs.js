const { listen, emit } = window.__TAURI__.event;

window.charts = {};

function createChart(graphId) {
  const chart = document.getElementById(graphId);
  if (!chart) return;

  const ctx = chart.getContext("2d");

  return new Chart(ctx, {
    type: "line",
    data: {
      labels: [],
      datasets: [
        {
          data: [],
          borderColor: "#f7f3ff",
          borderWidth: 2,
          fill: false,
          tension: 0.1,
        },
      ],
    },
    options: {
      animation: false,
      responsive: true,
      maintainAspectRatio: false,
      plugins: {
        legend: {
          display: false,
        },
        title: {
          display: false,
        },
      },
      scales: {
        x: {
          ticks: {
            color: "#f7f3ff",
            font: {
              family: "Montserrat, Arial, sans-serif",
              size: 12,
              weight: "normal",
            },
          },
          title: {
            display: true,
            text: "",
            color: "#f7f3ff",
            font: {
              family: "Montserrat, Arial, sans-serif",
              size: 12,
              weight: "bold",
            },
          },
        },
        y: {
          beginAtZero: true,
          ticks: {
            color: "#f7f3ff",
            font: {
              family: "Montserrat, Arial, sans-serif",
              size: 12,
              weight: "normal",
            },
          },
          title: {
            display: true,
            text: "",
            color: "#f7f3ff",
            font: {
              family: "Montserrat, Arial, sans-serif",
              size: 12,
              weight: "bold",
            },
          },
        },
      },
      elements: {
        point: {
          radius: 0,
        },
      },
    },
  });
}

window.charts.chart1 = createChart("chart1");
window.charts.chart2 = createChart("chart2");
window.charts.chart3 = createChart("chart3");
window.charts.chart4 = createChart("chart4");

window.addEventListener("resize", function () {
  Object.values(window.charts).forEach((chart) => {
    chart.resize();
  });
});

/// Search bar functionality for dashboard

const searchInput = document.querySelector(".search");

searchInput.addEventListener("input", () => {
  const query = searchInput.value.toLowerCase();
  const cards = document.querySelectorAll(".card");
  const rows = document.querySelectorAll(".info-row");

  // Filter cards in dashboard
  cards.forEach((card) => {
    const title = card.querySelector("h3").textContent.toLowerCase();
    const match = title.includes(query);
    card.style.display = match ? "flex" : "none";
  });

  // Filter PID rows in edit screen
  rows.forEach((row) => {
    const name = row.querySelector(".name").textContent.toLowerCase();
    const match = name.includes(query);
    const group = row.closest(".pid-group");
    group.style.display = match ? "block" : "none";
  });
});

const decodeVinBtn = document.getElementById("vin-decode-start");
const vinInput = document.querySelector(".vin-input");
const vinExportBtn = document.getElementById("vin-export");
const vinLabel = document.getElementById("vin-full");
const vehicleName = document.getElementById("vehicle-title");

decodeVinBtn.addEventListener("click", async function (event) {
  const query = vinInput.value.trim();
  if (query !== "") {
    // request backend to decode vin
    await emit("decode-vin", query);
  }
});

/// Toggle different screens when sidebar buttons are clicked

const screens = document.querySelectorAll(".screen");
const buttons = document.querySelectorAll(".sidebar-button");

buttons.forEach((button) => {
  // Default screen is dashboard
  if (button.dataset.screen == "dashboard") {
    button.classList.add("active-button");
  }

  button.addEventListener("click", () => {
    const target = button.dataset.screen;

    screens.forEach((screen) => {
      screen.style.display =
        screen.id === target
          ? screen.classList.contains("grid")
            ? "flex"
            : "flex"
          : "none";
    });

    // Show different background color for clicked button
    buttons.forEach((btn) => btn.classList.remove("active-button"));
    button.classList.add("active-button");
  });
});

/// Clicking cards makes them not report data and dims them

const dashboard = document.getElementById("dashboard-cards");

dashboard.addEventListener("click", (e) => {
  const card = e.target.closest(".card");
  if (!card) return;

  if (card.classList.contains("dimmed")) {
    // undim and set to first element
    card.classList.remove("dimmed");
    dashboard.insertBefore(card, dashboard.firstChild);
  } else {
    card.classList.add("dimmed");
    dashboard.appendChild(card);
  }
});

/// Refresh serial ports

const refreshButton = document.getElementById("refresh-serial-ports");
const refreshIcon = refreshButton.querySelector("svg");

const demoModePortOption = document.createElement("li");

refreshButton.addEventListener("click", () => {
  refreshIcon.classList.add("spinning");

  emit("get-serial-ports");
});

refreshIcon.addEventListener("animationend", () => {
  refreshIcon.classList.remove("spinning");
});
