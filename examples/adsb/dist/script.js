const UPDATE_INTERVAL_MS = 1000;

let autozoom_in_progress = false;
let autozoom = true;
let map = null;
let markers_group = null;

window.onload = (event) => {
    map = L.map('map').locate({setView: true, maxZoom: 10})
        .on('locationfound', initialize_aircraft_poll)
        .on('locationerror', initialize_aircraft_poll);
    L.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', {
        maxZoom: 19,
        attribution: '&copy; <a href="http://www.openstreetmap.org/copyright">OpenStreetMap</a>'
    }).addTo(map);
    markers_group = new L.LayerGroup().addTo(map);
};

var aeroplane_icon = L.divIcon({
    html: '<i class="bx bxs-plane" style="font-size: 40"></i>',
    iconSize: [40, 40],
    iconAnchor: [20, 20],
});


let aircraft_poll_state = 0;
function initialize_aircraft_poll() {
    map.on('zoomstart', function() {
        if (!autozoom_in_progress) {
            autozoom = false;
        }
    }).on('dragstart', function() {
        autozoom = false;
    });
    aircraft_poll_state = 1;
    aircraft_poll();
    setInterval(aircraft_poll, UPDATE_INTERVAL_MS);
}

let fg = null;
let tracker_block = null;
function aircraft_poll() {
    switch(aircraft_poll_state) {
    case 0: // uninitialized
        break;
    case 1: // Not connected
        fetch_fg().then((result) => {
            if(markers_group) {
                markers_group.clearLayers();
            }
            fg = result['fg'];
            tracker_block = result['tracker_block'];
            aircraft_poll_state = 2;
        }).catch((error) => {
            console.error(error);
        });
        break;
    case 2: // Connected
        fetch_aircrafts(tracker_block).then((register) => {
            update_aircrafts(register);
        }).catch((error) => {
            console.error(error);
            aircraft_poll_state = 1;
        });
        break;
    }
}

let aircraft_markers = {};
function update_aircrafts(register) {
    let cur_aircraft_positions = [];
    for(let icao in register.register) {
        let value = register.register[icao];
        if(value.positions.length > 0) {
            let pos_arr = [];
            for(let i = 0; i < value.positions.length; i++) {
                let pos = [value.positions[i].position.latitude,
                           value.positions[i].position.longitude];
                if(value.positions[i].position.altitude) {
                    pos.push(value.positions[i].position.altitude);
                }
                pos_arr.push(pos);
            }
            let cur_pos = pos_arr[pos_arr.length-1];
            console.log(cur_pos);
            cur_aircraft_positions.push(cur_pos);
            let cur_velocity = null;
            if(value.velocities.length > 0) {
                cur_velocity = value.velocities[value.velocities.length-1].velocity;
            }
            let ground_speed = (cur_velocity && cur_velocity.ground_speed.toFixed(2)) || "N/A";
            let heading = (cur_velocity && cur_velocity.heading) || "N/A";
            let vertical_rate = (cur_velocity && cur_velocity.vertical_rate) || "N/A";
            let vertical_rate_source = (cur_velocity && cur_velocity.vertical_rate_source) || "N/A";
            let last_updated = new Date(value.last_seen.secs_since_epoch*1e3+value.last_seen.nanos_since_epoch/1e6);
            let popup_content = `
                <center><h3>${icao.toUpperCase()}</h3></center>
                <table>
                <tr><td>Callsign:</td><td>${value.callsign}</td></tr>
                <tr><td>Last seen:</td><td>${last_updated.toLocaleString()}</td></tr>
                <tr><td>Emitter category:</td><td>${value.emitter_category}</td></tr>
                <tr><td>Altitude:</td><td>${cur_pos[2]} ft</td></tr>
                <tr><td>Latitude:</td><td>${cur_pos[0].toFixed(4)}</td></tr>
                <tr><td>Longitude:</td><td>${cur_pos[1].toFixed(4)}</td></tr>
                <tr><td>Ground speed:</td><td>${ground_speed} kt</td></tr>
                <tr><td>Vertical rate:</td><td>${vertical_rate} ft/min</td></tr>
                <tr><td>Vertical rate source:</td><td>${vertical_rate_source}</td></tr>
                </table>`;
            let angle = (cur_velocity && cur_velocity.heading) || 0;
            if(icao in aircraft_markers) {
                // Update position
                aircraft_markers[icao].marker
                    .setLatLng(pos_arr[pos_arr.length-1])
                    .setRotationAngle(angle)
                    .setPopupContent(popup_content)
                    .on("popupopen", (event) => {
                        aircraft_markers[icao].pathline.setStyle({opacity: 1.0});
                    })
                    .on("popupclose", (event) => {
                        aircraft_markers[icao].pathline.setStyle({opacity: 0.2});
                    });
                aircraft_markers[icao].pathline
                    .setLatLngs(pos_arr);
            } else {
                let marker = L.marker(pos_arr[pos_arr.length-1], {
                    icon: aeroplane_icon,
                    rotationAngle: angle,
                })
                    .bindPopup(popup_content, { minWidth: 300 })
                    .on("popupopen", (event) => {
                        aircraft_markers[icao].pathline.setStyle({opacity: 1.0});
                    })
                    .on("popupclose", (event) => {
                        aircraft_markers[icao].pathline.setStyle({opacity: 0.2});
                    })
                    .addTo(markers_group);
                let pathline = L.polyline(pos_arr, {
                    color: 'red',
                    opacity: 0.2,
                }).addTo(markers_group);
                aircraft_markers[icao] = {
                    marker: marker,
                    pathline: pathline,
                };
                
            }
        }
    }
    // Remove the aircraft we had but are not in the new records
    for(let marker in aircraft_markers) {
        if(!(marker in register.register)) {
            // Remove
            markers_group.removeLayer(aircraft_markers[marker].marker);
            markers_group.removeLayer(aircraft_markers[marker].pathline);
            delete aircraft_markers[marker];
        }
    }

    
    if(cur_aircraft_positions.length > 0 && autozoom) {
        autozoom_in_progress = true;
        map.flyToBounds(cur_aircraft_positions, { maxZoom: 10 });
        autozoom_in_progress = false;
    }
}

function fetch_aircrafts(tracker_block) {
    let ctrl_port = tracker_block.message_inputs.indexOf("ctrl_port");
    let url = `http://localhost:1337/api/fg/0/block/${tracker_block.id}/call/${ctrl_port}/`;
    let promise = fetch(url).then((response) => {
        if (!response.ok) {
            throw new Error(`HTTP error! Status: ${response.status}`);
        }
        return response.json();
    }).then((response) => {
        return JSON.parse(response.String);
    });
    return promise;
}

function fetch_fg() {
    let promise = fetch('http://localhost:1337/api/fg/0/').then((response) => {
        if (!response.ok) {
            throw new Error(`HTTP error! Status: ${response.status}`);
        }
        return response.json();
    }).then((response) => {
        fg = response;
        for(let i = 0; i < fg.blocks.length; i++) {
            if(fg.blocks[i].type_name == "Tracker") {
                tracker_block = fg.blocks[i];
            }
        }
        return {'fg': fg, 'tracker_block': tracker_block};
    });
    return promise;
}
