import { memo, type ReactNode } from "react";
import { motion, MotionConfig, type HTMLMotionProps } from "motion/react";
import { useReducedMotionPreference } from "./use-reduced-motion";

export const galleryTransition = { duration: .28, ease: [.22, 1, .36, 1] as [number, number, number, number] };

/** One layout animation policy for gallery images, section images and album covers. */
export function GalleryMotion({ children }: { children: ReactNode }) {
  const reducedMotion = useReducedMotionPreference();
  return <MotionConfig reducedMotion="never" transition={{ layout: { ...galleryTransition, duration: reducedMotion ? 0 : galleryTransition.duration } }}>{children}</MotionConfig>;
}

export const GalleryLayoutItem = memo(function GalleryLayoutItem(props: HTMLMotionProps<"div">) {
  return <motion.div layout initial={false} {...props} />;
});
