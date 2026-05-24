import { useMemo } from 'react';
import { usePluginStore } from '../../stores/pluginStore';

export default function LoadingScreen() {
  const { pluginChain, restoreTargetCount, isChainInitializing } = usePluginStore();

  const { statusLine, progressLine } = useMemo(() => {
    const target = restoreTargetCount ?? 0;
    const restored = restoreTargetCount == null
      ? pluginChain.length
      : Math.min(pluginChain.length, restoreTargetCount);

    const status = isChainInitializing ? 'Restoring session' : 'Starting audio engine';
    const progress = target > 0
      ? `Plugins ${restored}/${target}`
      : 'Preparing audio graph';

    return { statusLine: status, progressLine: progress };
  }, [pluginChain.length, restoreTargetCount, isChainInitializing]);

  return (
    <div className="rh-loading-screen" aria-live="polite" aria-busy="true">
      <div className="rh-loading-card">
        <div className="rh-loading-brand">
          <img className="rh-loading-logo" src="/logo.png" alt="ReLightHost" />
          <div className="rh-loading-title">ReLightHost</div>
        </div>
        <div className="rh-loading-orb">
          <span className="rh-loading-ring" />
          <span className="rh-loading-ring rh-loading-ring--alt" />
          <span className="rh-loading-ring rh-loading-ring--soft" />
        </div>
        <div className="rh-loading-bars" aria-hidden="true">
          <span />
          <span />
          <span />
          <span />
          <span />
        </div>
        <div className="rh-loading-status">
          <div className="rh-loading-line">{statusLine}...</div>
          <div className="rh-loading-sub">{progressLine}</div>
        </div>
      </div>
    </div>
  );
}
