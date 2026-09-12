import { RotateCcw, SlidersHorizontal } from "lucide-react";
import { Button, Panel } from "../components";
import type { Studio } from "../hooks/useStudio";
import { DEFAULT_SETTINGS, PROFILES } from "../config/profiles";
import { RangeControl } from "../RangeControl";

export function SettingsPanel({ studio }: { studio: Studio }) {
  const { settings, setSettings, activeProfile, applyProfile, updateSetting, isRunning } = studio;
  return (
    <Panel>
      <div className="panel__heading panel__heading--compact panel__heading--action">
        <span className="icon-box"><SlidersHorizontal size={19} /></span>
        <div><span className="step-label">Rendu</span><h2>Réglages</h2></div>
        <Button
          variant="ghost"
          className="icon-button"
          icon={<RotateCcw size={15} />}
          aria-label="Réinitialiser les réglages"
          title="Réinitialiser les réglages"
          disabled={isRunning}
          onClick={() => setSettings(DEFAULT_SETTINGS)}
        />
      </div>
      <div className="profile-picker" role="group" aria-label="Profil de rendu">
        {PROFILES.map((profile) => {
          const ProfileIcon = profile.icon;
          return (
            <button
              key={profile.id}
              type="button"
              className={`profile-button ${activeProfile === profile.id ? "profile-button--active" : ""}`}
              aria-pressed={activeProfile === profile.id}
              disabled={isRunning}
              onClick={() => applyProfile(profile.id)}
            >
              <ProfileIcon size={16} aria-hidden="true" />
              <span><strong>{profile.label}</strong><small>{profile.detail}</small></span>
            </button>
          );
        })}
      </div>
      <div className="sliders">
        <RangeControl label="Luminosité" value={settings.brightness} min={10} max={100} suffix="%" disabled={isRunning} onChange={(value) => updateSetting("brightness", value)} />
        <RangeControl label="Saturation" value={settings.saturation} min={40} max={150} suffix="%" disabled={isRunning} onChange={(value) => updateSetting("saturation", value)} />
        <RangeControl label="Réactivité" value={settings.reactivity} min={10} max={100} suffix="%" disabled={isRunning} onChange={(value) => updateSetting("reactivity", value)} />
      </div>
      <div className="render-options">
        <div className="cadence-control">
          <span>Cadence cible</span>
          <div className="option-picker" role="group" aria-label="Cadence cible">
            {[30, 45, 60].map((fps) => (
              <button
                key={fps}
                type="button"
                aria-pressed={settings.fps === fps}
                className={settings.fps === fps ? "option-button option-button--active" : "option-button"}
                disabled={isRunning}
                onClick={() => updateSetting("fps", fps)}
              >
                {fps}
              </button>
            ))}
          </div>
        </div>
        <button
          type="button"
          className="toggle-row"
          role="switch"
          aria-checked={settings.blackBarDetection}
          disabled={isRunning}
          onClick={() => updateSetting("blackBarDetection", !settings.blackBarDetection)}
        >
          <span><strong>Bandes noires automatiques</strong></span>
          <span className={`toggle-indicator ${settings.blackBarDetection ? "toggle-indicator--on" : ""}`} aria-hidden="true"><span /></span>
        </button>
      </div>
      <details className="advanced-settings">
        <summary>Réglages avancés</summary>
        <div className="sliders">
          <RangeControl label="Luminosité max" value={settings.maxLuminosity} min={20} max={100} suffix="%" disabled={isRunning} onChange={(value) => updateSetting("maxLuminosity", value)} />
          <RangeControl label="Portée du cône" value={settings.edgeDepth} min={5} max={30} suffix="" disabled={isRunning} onChange={(value) => updateSetting("edgeDepth", value)} />
        </div>
      </details>
    </Panel>
  );
}
