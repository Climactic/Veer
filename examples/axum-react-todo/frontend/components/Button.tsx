import type { ButtonHTMLAttributes } from "react";

type Variant = "primary" | "secondary" | "danger";

type Props = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: Variant;
};

export default function Button({
  variant = "primary",
  className,
  ...rest
}: Props) {
  const classes = ["btn", `btn-${variant}`, className].filter(Boolean).join(" ");
  return <button className={classes} {...rest} />;
}
