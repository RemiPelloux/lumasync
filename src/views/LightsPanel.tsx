import { Check, House, Lightbulb, Plus, Radio, RefreshCw } from "lucide-react";
import { Button, Panel } from "../components";
import type { Studio } from "../hooks/useStudio";

export function LightsPanel({ studio }: { studio: Studio }) {
  const { areas, areaId, rooms, roomId, setRoomId, assignments, mappingMode, zoneOptions, loading, activeArea, activeRoom, chooseArea, createAreaFromRoom, assignZone, applyMappingMode, isRunning } = studio;
  return (
    <Panel>
      <div className="panel__heading panel__heading--compact panel__heading--action">
        <span className="icon-box"><Lightbulb size={19} /></span>
        <div><span className="step-label">Zone Hue</span><h2>Lumières</h2></div>
        <Button variant="ghost" className="icon-button" icon={<RefreshCw size={16} />} aria-label="Actualiser les lampes et les écrans" title="Actualiser les lampes et les écrans" disabled={isRunning || studio.isBusy} busy={loading === "data"} onClick={() => void studio.loadBridgeData()} />
      </div>

      <div className="choice-stack">
        {areas.map((area) => (
          <button
            key={area.id}
            type="button"
            className={`choice-row ${area.id === areaId ? "choice-row--selected" : ""}`}
            onClick={() => chooseArea(area)}
            aria-pressed={area.id === areaId}
            disabled={isRunning || studio.isBusy}
          >
            <span><strong>{area.name}</strong><small>{area.channels.length} canaux</small></span>
            {area.id === areaId && <Radio size={17} aria-hidden="true" />}
          </button>
        ))}
      </div>

      {areas.length === 0 && rooms.length === 0 && <p className="empty-state">{studio.isBusy && !studio.dataLoaded ? "Recherche des lampes…" : "Aucune zone disponible."}</p>}

      {areas.length === 0 && rooms.length > 0 && (
        <div className="room-recovery">
          <div className="room-recovery__message">
            <House size={18} aria-hidden="true" />
            <div>
              <strong>Pièce Hue détectée</strong>
              <p>« {activeRoom?.name ?? "Cette pièce"} » est une pièce. Le streaming nécessite une zone Entertainment séparée.</p>
            </div>
          </div>
          <div className="choice-stack" aria-label="Pièce à utiliser">
            {rooms.map((room) => (
              <button
                key={room.id}
                type="button"
                className={`choice-row ${room.id === roomId ? "choice-row--selected" : ""}`}
                onClick={() => setRoomId(room.id)}
                aria-pressed={room.id === roomId}
                disabled={loading === "create-area"}
              >
                <span><strong>{room.name}</strong><small>{room.lightCount} lampe{room.lightCount > 1 ? "s" : ""}</small></span>
                {room.id === roomId && <Check size={17} aria-hidden="true" />}
              </button>
            ))}
          </div>
          <p className="room-recovery__note">La pièce reste intacte. LumaSync ajoute seulement la configuration requise au pont.</p>
          <Button
            variant="primary"
            icon={<Plus size={18} />}
            busy={loading === "create-area"}
            disabled={!roomId}
            onClick={() => void createAreaFromRoom()}
          >
            Créer depuis {activeRoom?.name ?? "cette pièce"}
          </Button>
        </div>
      )}

      {activeArea && (
        <details className="mapping-list">
          <summary>Position des lumières <span>{assignments.length} canaux</span></summary>
          <div className="mapping-content">
          <div className="mapping-mode">
            <div className="cadence-control">
              <span>Disposition</span>
              <div className="option-picker" role="group" aria-label="Disposition des lumières">
                {([
                  { id: "corners", label: "Angles" },
                  { id: "edges", label: "Bords" },
                ] as const).map((mode) => (
                  <button
                    key={mode.id}
                    type="button"
                    aria-pressed={mappingMode === mode.id}
                    className={mappingMode === mode.id ? "option-button option-button--active" : "option-button"}
                    disabled={isRunning}
                    onClick={() => applyMappingMode(mode.id)}
                  >
                    {mode.label}
                  </button>
                ))}
              </div>
            </div>
          </div>
          {activeArea.channels.map((channel) => {
            const current = assignments.find((item) => item.channelId === channel.channelId)?.zone;
            return (
              <div className="mapping-row" key={channel.channelId}>
                <span className="light-name">{channel.name}</span>
                <div
                  className={`zone-picker ${mappingMode === "corners" ? "zone-picker--corners" : ""}`}
                  role="group"
                  aria-label={`Position de ${channel.name}`}
                >
                  {zoneOptions.map((zone) => (
                    <button
                      key={zone.id}
                      type="button"
                      className={current === zone.id ? "zone-button zone-button--active" : "zone-button"}
                      onClick={() => assignZone(channel.channelId, zone.id)}
                      aria-label={zone.label}
                      title={zone.label}
                      aria-pressed={current === zone.id}
                      disabled={isRunning}
                    >{zone.short}</button>
                  ))}
                </div>
              </div>
            );
          })}
          </div>
        </details>
      )}
    </Panel>
  );
}
