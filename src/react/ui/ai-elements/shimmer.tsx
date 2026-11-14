import { type CSSProperties, type ElementType, type JSX, memo, useMemo } from "react";
import { motion } from "motion/react";
import { cn } from "../cn";

export type TextShimmerProps = { children: string; as?: ElementType; className?: string; duration?: number; spread?: number };

const ShimmerComponent = ({ children, as: Component = "p", className, duration = 2, spread = 2 }: TextShimmerProps) => {
  const MotionComponent = useMemo(() => motion.create(Component as keyof JSX.IntrinsicElements), [Component]);
  const dynamicSpread = useMemo(() => (children?.length ?? 0) * spread, [children, spread]);
  return (
    <MotionComponent
      animate={{ backgroundPosition: "0% center" }}
      className={cn(
        "relative inline-block bg-[length:250%_100%,auto] bg-clip-text text-transparent",
        "[--bg:linear-gradient(90deg,#0000_calc(50%-var(--spread)),var(--surface),#0000_calc(50%+var(--spread)))] [background-repeat:no-repeat,padding-box]",
        className,
      )}
      initial={{ backgroundPosition: "100% center" }}
      style={{ "--spread": `${dynamicSpread}px`, backgroundImage: "var(--bg), linear-gradient(var(--text-3), var(--text-3))" } as CSSProperties}
      transition={{ repeat: Number.POSITIVE_INFINITY, duration, ease: "linear" }}
    >
      {children}
    </MotionComponent>
  );
};

export const Shimmer = memo(ShimmerComponent);
