import { getRowThumbnail } from "../api";
import { ImageLoader } from "./image-loader";

/** 全应用共享的缩略图加载器（队列限流 + LRU 缓存）。 */
export const thumbnails = new ImageLoader(getRowThumbnail, 8, 480, "image/png");
