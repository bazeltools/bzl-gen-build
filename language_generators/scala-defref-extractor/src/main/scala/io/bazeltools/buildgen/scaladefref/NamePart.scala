package io.bazeltools.buildgen.scaladefref

import cats.data.NonEmptyList
import io.bazeltools.buildgen.shared.Entity

sealed abstract class NamePart {
  def toEntity: Option[Entity]
}

object NamePart {
  case class Package(entity: Entity) extends NamePart {
    def toEntity = Some(entity)
  }
  case class Defn(entity: Entity) extends NamePart {
    def toEntity = Some(entity)
  }
  case object Anonymous extends NamePart {
    def toEntity = None
  }

  /** a list of all packages a scope with a vector of NameParts a non-local
    * reference could refer to
    */
  def referencePackages(path: Seq[NamePart]): NonEmptyList[Entity] = {
    def loop(pathList: List[NamePart]): List[Entity] =
      pathList match {
        case NamePart.Package(ent) :: tail =>
          // package foo {
          //   package bar {
          //     // refer to name x could be x, foo.x, foo.bar.x
          //   }
          // }
          ent :: loop(tail).map(ent / _)
        case _ => Nil
      }

    NonEmptyList(Entity.empty, loop(path.toList))
  }
}
