import { Button, Popconfirm, Space, Tooltip, Typography, theme } from 'antd';
import { PlusOutlined, DeleteOutlined } from '@ant-design/icons';

const { Text } = Typography;

interface ChainToolbarProps {
  isChainInitializing: boolean;
  addLocked: boolean;
  isDeleteAllBusy: boolean;
  pluginChainLength: number;
  onAddPlugin: () => void;
  onDeleteAll: () => void;
}

export default function ChainToolbar({
  isChainInitializing,
  addLocked,
  isDeleteAllBusy,
  pluginChainLength,
  onAddPlugin,
  onDeleteAll,
}: ChainToolbarProps) {
  const { token } = theme.useToken();

  return (
    <div
      className="glass-panel"
      style={{
        marginBottom: 0,
        padding: '14px 18px',
        borderRadius: `${token.borderRadiusLG * 1.25}px ${token.borderRadiusLG * 1.25}px 0 0`,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        gap: 16,
        flexWrap: 'wrap',
        background: 'var(--rh-surface-soft-gradient)',
        border: '1px solid var(--rh-surface-soft-border-strong)',
        borderBottom: 'none',
        boxShadow: 'var(--rh-chain-toolbar-shadow)',
      }}
    >
      <Space orientation="vertical" size={2}>
        <Text
          strong
          style={{
            fontSize: 20,
            fontWeight: 700,
            letterSpacing: '-0.02em',
            color: token.colorText,
            margin: 0,
            display: 'block',
          }}
        >
          Signal chain
        </Text>
        <Text style={{ fontSize: 12, color: token.colorTextTertiary, margin: 0 }}>
          Drag cards to reorder · right-click empty area to add
        </Text>
      </Space>

      <Space size="middle" wrap>
        <Tooltip title={isChainInitializing ? 'Preparing…' : 'Add plugin'}>
          <Button
            type="primary"
            icon={<PlusOutlined />}
            size="middle"
            className="btn-pill"
            loading={addLocked}
            disabled={addLocked}
            onClick={onAddPlugin}
            aria-label={isChainInitializing ? 'Preparing plugin library' : 'Add plugin'}
          >
            Add Plugin
          </Button>
        </Tooltip>

        <Popconfirm
          title="Remove all plugins from the chain?"
          onConfirm={onDeleteAll}
          okText="Remove"
          cancelText="Cancel"
        >
          <Button
            size="middle"
            type="default"
            icon={<DeleteOutlined />}
            loading={isDeleteAllBusy}
            disabled={pluginChainLength === 0 || isDeleteAllBusy || isChainInitializing}
            className="btn-pill btn-tonal"
            style={{
              borderColor: 'rgba(99,103,255,0.2)',
              color: token.colorTextSecondary,
            }}
          >
          </Button>
        </Popconfirm>
      </Space>
    </div>
  );
}
