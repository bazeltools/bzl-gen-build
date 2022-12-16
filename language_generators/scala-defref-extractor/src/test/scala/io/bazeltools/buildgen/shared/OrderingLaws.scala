package io.bazeltools.buildgen.shared

import org.scalacheck.{Gen, Prop}
import Prop.forAll

object OrderingLaws {
  def orderingLaws[A: Ordering](ga: Gen[A]): Prop = {
    // a <= b and b <= c implies a <= c
    val ord = implicitly[Ordering[A]]

    forAll(ga, ga, ga) { (a, b, c) =>
      if (ord.lteq(a, b) && ord.lteq(b, c)) {
        Prop(ord.lteq(a, c)).label(s"$a <= $b && $b <= $c should imply $a <= $c")
      }
      else Prop(true)
    } && forAll(ga) { a =>
      Prop(ord.equiv(a, a)).label(s"$a == $a")
    } && forAll(ga, ga) { (a, b) =>
      if (ord.lteq(a, b) && ord.lteq(b, a)) {
        Prop(ord.equiv(a, b)).label(s"$a <= $b && $b <= $a implies $a == $b")
      }
      else Prop(true)
    } && forAll(ga, ga) { (a, b) =>
      Prop(ord.lteq(a, b) || ord.lteq(b, a)).label(s"$a <= $b || $b <= $a")
    }
  }
}