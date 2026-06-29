export interface AdvancedAction {
  command: string;
  labelKey: string;
  destructive?: boolean;
}

export const ADVANCED_ACTIONS: AdvancedAction[] = [
  { command: "读取FlashID", labelKey: "action.readFlashId" },
  { command: "读取Flash信息", labelKey: "action.readFlashInfo" },
  { command: "读取Chip信息", labelKey: "action.readChipInfo" },
  { command: "读取Capability", labelKey: "action.readCapability" },
  { command: "测试设备", labelKey: "action.testDevice" },
  { command: "重启设备", labelKey: "action.rebootDevice" },
  { command: "进入Maskrom", labelKey: "action.enterMaskrom" },
  { command: "切换存储", labelKey: "action.switchStorage" },
  { command: "清空序列号", labelKey: "action.clearSerial" },
  { command: "检测安全模式", labelKey: "action.detectSecureMode" },
  { command: "导出串口日志", labelKey: "action.exportSerialLog" },
  { command: "获取当前存储", labelKey: "action.getCurrentStorage" },
  { command: "导出镜像", labelKey: "action.exportImage" },
  { command: "擦除扇区", labelKey: "action.eraseSector", destructive: true },
  { command: "擦除所有", labelKey: "action.eraseAll", destructive: true },
  { command: "切换USB3", labelKey: "action.switchUsb3" },
];
