export function triggerRipple(
  docRoot: Element,
  left: number,
  top: number,
  className: string,
  animation: string
) {
  const ripple = document.createElement("div");

  ripple.className = className;
  ripple.style.left = left.toString() + "px";
  ripple.style.top = top.toString() + "px";

  docRoot.appendChild(ripple);

  ripple.style.animation = animation;
  ripple.onanimationend = () => {
    docRoot.removeChild(ripple);
  };
}
