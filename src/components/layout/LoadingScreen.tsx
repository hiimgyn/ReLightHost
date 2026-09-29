import { useMemo } from 'react';
import { usePluginStore } from '../../stores/pluginStore';

export default function LoadingScreen() {
  const { pluginChain, restoreTargetCount, restoreProgressCount, isChainInitializing } = usePluginStore();

  const { statusLine, progressLine } = useMemo(() => {
    const target = restoreTargetCount ?? 0;
    // See Header.tsx's restoredCount comment: pluginChain.length alone stays
    // at 0 for the whole restore, so it's combined with the live per-plugin
    // backend event count instead.
    const restored = restoreTargetCount == null
      ? pluginChain.length
      : Math.min(Math.max(pluginChain.length, restoreProgressCount), restoreTargetCount);

    const status = isChainInitializing ? 'Restoring session' : 'Starting audio engine';
    const progress = target > 0
      ? `Plugins ${restored}/${target}`
      : 'Preparing audio graph';

    return { statusLine: status, progressLine: progress };
  }, [pluginChain.length, restoreTargetCount, restoreProgressCount, isChainInitializing]);

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
