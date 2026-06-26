# Skill Hub 蓝色图标生成指南

## 已生成文件
- ✅ `src-tauri/icons/icon.svg` - 蓝色主题的 SVG 图标

## 需要生成的文件
- `src-tauri/icons/icon.png` (512x512px)
- `src-tauri/icons/icon.ico` (多尺寸)

## 方法 1：使用在线工具（推荐）

### 步骤 1：SVG 转 PNG
1. 打开 https://cloudconvert.com/svg-to-png
2. 上传 `src-tauri/icons/icon.svg`
3. 设置尺寸为 512x512
4. 下载并保存为 `src-tauri/icons/icon.png`

### 步骤 2：PNG 转 ICO
1. 打开 https://cloudconvert.com/png-to-ico
2. 上传刚生成的 `icon.png`
3. 下载并保存为 `src-tauri/icons/icon.ico`

## 方法 2：使用 Inkscape（本地）
```bash
inkscape icon.svg --export-type=png --export-width=512 --export-height=512 -o icon.png
```

## 方法 3：使用 ImageMagick
```bash
magick convert -background none -size 512x512 icon.svg icon.png
magick convert icon.png -define icon:auto-resize=256,128,96,64,48,32,16 icon.ico
```

## 方法 4：使用 Node.js (sharp)
```bash
npm install sharp
node -e "require('sharp')('icon.svg').resize(512,512).png().toFile('icon.png')"
```

## 图标设计说明

### 设计概念
- **层叠卡片**：代表多个技能模块的管理和切换
- **蓝色渐变**：使用项目主题色 #4f7cff
- **现代简洁**：圆角设计，符合现代 UI 趋势
- **深度感**：通过阴影和透明度营造立体感

### 颜色方案
- 主蓝色：#4f7cff
- 渐变蓝：#5d8aff
- 白色叠层：带透明度的白色卡片

### 图标元素
1. 蓝色渐变背景（圆角矩形）
2. 三层白色半透明卡片（轻微旋转角度）
3. 装饰线条和圆点
4. 中心六边形标识

## 构建后
图标会自动应用到：
- Windows 任务栏图标
- 应用窗口标题栏图标
- 安装包图标
